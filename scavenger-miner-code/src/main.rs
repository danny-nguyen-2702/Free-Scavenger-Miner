use ashmaize::{Rom, RomGenerationType, hash};
use rayon::prelude::*;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::env;
use std::fs;
use std::path::Path;
use std::io::Write;

// Windows-specific CPU detection for processor groups (handles >64 logical processors and multi-socket systems)
#[cfg(windows)]
fn get_total_logical_processors() -> usize {
    // Manually declare Windows API functions for processor group support
    #[link(name = "kernel32")]
    extern "system" {
        fn GetActiveProcessorGroupCount() -> u16;
        fn GetActiveProcessorCount(GroupNumber: u16) -> u32;
    }

    const ALL_PROCESSOR_GROUPS: u16 = 0xFFFF;

    unsafe {
        // Try to get total processors across all groups (Windows 7+)
        let total = GetActiveProcessorCount(ALL_PROCESSOR_GROUPS);
        if total > 0 {
            return total as usize;
        }

        // Fallback: Sum processors in each group
        let group_count = GetActiveProcessorGroupCount();
        if group_count > 0 {
            let mut total_cpus = 0u32;
            for group in 0..group_count {
                total_cpus += GetActiveProcessorCount(group);
            }

            if total_cpus > 0 {
                return total_cpus as usize;
            }
        }

        // Final fallback to num_cpus
        num_cpus::get()
    }
}

// Windows-specific thread affinity setting for processor groups
#[cfg(windows)]
fn set_thread_processor_group_affinity(thread_index: usize) {
    #[repr(C)]
    #[allow(non_snake_case)]  // Windows API requires exact field names
    struct GROUP_AFFINITY {
        Mask: usize,
        Group: u16,
        Reserved: [u16; 3],
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn SetThreadGroupAffinity(
            hThread: *mut std::ffi::c_void,
            GroupAffinity: *const GROUP_AFFINITY,
            PreviousGroupAffinity: *mut GROUP_AFFINITY,
        ) -> i32;
        fn GetActiveProcessorGroupCount() -> u16;
        fn GetActiveProcessorCount(GroupNumber: u16) -> u32;
    }

    unsafe {
        let group_count = GetActiveProcessorGroupCount() as usize;
        if group_count <= 1 {
            // Single processor group, no need to set affinity
            return;
        }

        // Distribute threads evenly across processor groups
        let group = (thread_index % group_count) as u16;
        let processors_in_group = GetActiveProcessorCount(group) as usize;

        // Set affinity to ALL processors in this group (not just one!)
        // This allows the OS to schedule the thread on any processor in the group
        // while preventing it from running on processors in other groups
        let mask = if processors_in_group >= 64 {
            !0usize  // All bits set
        } else {
            (1usize << processors_in_group) - 1  // Set bits 0 to processors_in_group-1
        };

        let affinity = GROUP_AFFINITY {
            Mask: mask,
            Group: group,
            Reserved: [0; 3],
        };

        SetThreadGroupAffinity(
            GetCurrentThread(),
            &affinity,
            std::ptr::null_mut(),
        );
    }
}

// Non-Windows platforms use num_cpus directly
#[cfg(not(windows))]
fn get_total_logical_processors() -> usize {
    num_cpus::get()
}

// Scavenger Mine configuration from the whitepaper
const ROM_SIZE: usize = 1_073_741_824; // 1GB
const PRE_SIZE: usize = 16_777_216; // 16MB
const MIXING_NUMBERS: usize = 4;
const NB_LOOPS: u32 = 8;
const NB_INSTRS: u32 = 256;

// Logging and export directories
const SOLUTIONS_DIR: &str = "solutions";
const LOGS_DIR: &str = "logs";
const DIFFICULT_TASKS_FILE: &str = "difficult_tasks.json";

// API endpoints (only need challenges and Scavenger submission for user-only mode)
const SCAVENGER_API_BASE: &str = "https://mine.defensio.io/api";

/// Difficult task record (challenge-wallet pair that's too hard to mine)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DifficultTask {
    wallet_address: String,
    challenge_id: String,
    marked_at: String,
    total_hashes: u64,
    mining_duration_secs: u64,
}

/// Response from challenge API (single challenge)
#[derive(Debug, serde::Deserialize)]
struct ChallengeResponse {
    challenge: Challenge,
    total_challenges: Option<u32>,
    starts_at: Option<String>,
    next_challenge_starts_at: Option<String>,
}

/// Challenge information from the API
#[derive(Debug, Clone, serde::Deserialize)]
struct Challenge {
    challenge_id: String,
    #[serde(default)]
    challenge_number: Option<u32>,
    #[serde(default)]
    day: Option<u32>,
    #[serde(default)]
    issued_at: Option<String>,
    difficulty: String,
    no_pre_mine: String,
    latest_submission: String,
    no_pre_mine_hour: String,
}

impl Challenge {
    /// Check if challenge is still active with 1-hour safety buffer
    /// A challenge is considered active only if: current_time + 1 hour < latest_submission
    /// This prevents mining challenges that might expire before solution is found
    fn is_active(&self) -> bool {
        match chrono::DateTime::parse_from_rfc3339(&self.latest_submission) {
            Ok(deadline) => {
                let now = chrono::Utc::now();
                // Add 1-hour buffer (3600 seconds) to current time
                // Challenge is active only if deadline is more than 1 hour away
                let safety_buffer = chrono::Duration::hours(1);
                let now_with_buffer = now + safety_buffer;
                now_with_buffer < deadline
            }
            Err(_) => {
                // If we can't parse the deadline, assume it's still active
                true
            }
        }
    }

    /// Count total zero bits in difficulty (more zeros = harder)
    /// Zero bits represent constraints - hash MUST have 0 at those positions
    fn count_required_zero_bits(&self) -> u32 {
        match hex::decode(&self.difficulty) {
            Ok(bytes) => {
                // Count total zero bits across all bytes
                bytes.iter().map(|b| b.count_zeros()).sum()
            }
            Err(_) => u32::MAX, // Invalid difficulty = hardest
        }
    }

    /// Count leading zero bits in difficulty (more leading zeros = easier)
    /// Leading zeros create consecutive pattern at start = easier to match
    fn count_leading_zero_bits(&self) -> u32 {
        match hex::decode(&self.difficulty) {
            Ok(bytes) => {
                let mut leading_zeros = 0u32;
                for byte in bytes.iter() {
                    let byte_leading = byte.leading_zeros();
                    leading_zeros += byte_leading;

                    // If this byte doesn't have all 8 bits as zero, stop counting
                    if byte_leading < 8 {
                        break;
                    }
                }
                leading_zeros
            }
            Err(_) => 0, // Invalid difficulty = no leading zeros
        }
    }

    /// Comprehensive comparison for optimal challenge selection
    /// Priority order:
    /// 1. Total zero bits (fewer = easier, since zeros are constraints)
    /// 2. Leading zero bits (more = easier, consecutive pattern at start)
    /// 3. Latest submission (thread-count dependent for optimization)
    /// 4. Challenge ID (deterministic tiebreaker)
    fn compare_for_selection(&self, other: &Challenge, num_threads: usize) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        // 1. Primary: Total zero bits (fewer zeros = easier)
        // Zero bits are constraints - hash must have 0s at those positions
        let a_zeros = self.count_required_zero_bits();
        let b_zeros = other.count_required_zero_bits();
        let zeros_cmp = a_zeros.cmp(&b_zeros); // Ascending order (fewer first)
        if zeros_cmp != Ordering::Equal {
            return zeros_cmp;
        }

        // 2. Secondary: Leading zero bits (more = easier)
        // Consecutive zeros at start are easier to match than scattered zeros
        let a_leading = self.count_leading_zero_bits();
        let b_leading = other.count_leading_zero_bits();
        let leading_cmp = b_leading.cmp(&a_leading); // Descending order (more first)
        if leading_cmp != Ordering::Equal {
            return leading_cmp;
        }

        // 3. Tertiary: Latest submission (thread-count dependent)
        // < 6 threads: prefer newer submissions (descending)
        // >= 6 threads: prefer older submissions (ascending) - less competition
        let time_cmp = if num_threads < 6 {
            other.latest_submission.cmp(&self.latest_submission) // Descending (newer first)
        } else {
            self.latest_submission.cmp(&other.latest_submission) // Ascending (older first)
        };
        if time_cmp != Ordering::Equal {
            return time_cmp;
        }

        // 4. Final: Challenge ID (deterministic tiebreaker)
        self.challenge_id.cmp(&other.challenge_id)
    }
}

/// Crypto receipt from Scavenger Mine API
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
struct CryptoReceipt {
    preimage: String,
    timestamp: String,
    signature: String,
}

/// Response from Scavenger Mine submission
#[derive(Debug, serde::Deserialize)]
struct ScavengerSubmitResponse {
    crypto_receipt: Option<CryptoReceipt>,
}

/// Solution record for export
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SolutionRecord {
    wallet_address: String,
    challenge_id: String,
    nonce: String,
    found_at: String,
    submitted_at: Option<String>,
    crypto_receipt: Option<CryptoReceipt>,
    status: String,
    #[serde(default)]
    error_message: Option<String>,
    #[serde(default)]
    retry_count: u32,
    #[serde(default)]
    last_retry_at: Option<String>,
}

/// ROM cache to avoid reinitializing for the same no_pre_mine
struct RomCache {
    rom: Option<Arc<Rom>>,
    no_pre_mine: String,
}

impl RomCache {
    fn new() -> Self {
        RomCache {
            rom: None,
            no_pre_mine: String::new(),
        }
    }

    fn get_or_create(&mut self, no_pre_mine: &str) -> Arc<Rom> {
        if self.no_pre_mine != no_pre_mine || self.rom.is_none() {
            println!("\n🔄 ROM cache miss - initializing new ROM...");
            println!("   no_pre_mine: {}...", &no_pre_mine[..16.min(no_pre_mine.len())]);
            let start = Instant::now();

            let rom = Rom::new(
                no_pre_mine.as_bytes(),
                RomGenerationType::TwoStep {
                    pre_size: PRE_SIZE,
                    mixing_numbers: MIXING_NUMBERS,
                },
                ROM_SIZE,
            );

            println!("   ✓ ROM initialized in {:.2?}\n", start.elapsed());

            self.rom = Some(Arc::new(rom));
            self.no_pre_mine = no_pre_mine.to_string();
        } else {
            println!("\n♻️  ROM cache hit - reusing existing ROM\n");
        }

        Arc::clone(self.rom.as_ref().unwrap())
    }
}

/// Optimized difficulty check using pre-decoded difficulty bytes
/// This avoids expensive hex decoding in the hot mining loop
fn check_difficulty(hash: &[u8; 64], diff_bytes: &[u8]) -> bool {
    let check_bytes = diff_bytes.len().min(hash.len());

    for i in 0..check_bytes {
        let hash_byte = hash[i];
        let diff_byte = diff_bytes[i];

        if (hash_byte & !diff_byte) != 0 {
            return false;
        }
    }

    true
}

/// Get current timestamp as ISO 8601 string
fn get_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();
    let datetime = chrono::DateTime::from_timestamp(now.as_secs() as i64, 0)
        .unwrap_or_default();
    datetime.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Setup output directories
fn setup_directories() -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(SOLUTIONS_DIR)?;
    fs::create_dir_all(LOGS_DIR)?;
    Ok(())
}

/// Log mining progress to file
fn log_mining_progress(message: &str) {
    let timestamp = get_timestamp();
    let log_message = format!("[{}] {}\n", timestamp, message);

    // Print to console
    print!("{}", log_message);
    std::io::stdout().flush().ok();

    // Write to log file
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{}/mining.log", LOGS_DIR))
    {
        let _ = file.write_all(log_message.as_bytes());
    }
}

/// Export solution to file
fn export_solution(record: &SolutionRecord) -> Result<(), Box<dyn std::error::Error>> {
    // Create filename: wallet_challenge.json (using full wallet address)
    let filename = format!(
        "{}/{}_{}.json",
        SOLUTIONS_DIR,
        record.wallet_address,
        record.challenge_id.replace("*", "").replace("/", "_")
    );

    let json = serde_json::to_string_pretty(record)?;
    fs::write(&filename, json)?;

    log_mining_progress(&format!("💾 Exported solution to: {}", filename));
    Ok(())
}


/// Update existing solution record
fn update_solution_record(record: &SolutionRecord) -> Result<(), Box<dyn std::error::Error>> {
    export_solution(record)
}

/// Get all failed solution files that need retry
fn get_failed_solutions() -> Vec<SolutionRecord> {
    let mut failed_solutions = Vec::new();

    if let Ok(entries) = fs::read_dir(SOLUTIONS_DIR) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() && entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(entry.path()) {
                        if let Ok(record) = serde_json::from_str::<SolutionRecord>(&content) {
                            // Only include failed submissions that should be retried
                            if record.crypto_receipt.is_none() &&
                               (record.status == "rejected" || record.status.starts_with("error:") || record.status == "failed") {

                                // Skip non-retriable errors
                                if let Some(ref error_msg) = record.error_message {
                                    let error_lower = error_msg.to_lowercase();

                                    // Don't retry if solution already exists (submitted elsewhere)
                                    if error_lower.contains("solution already exists") ||
                                       error_lower.contains("already exists") {
                                        continue;
                                    }

                                    // Don't retry if the challenge already closed and the latest submission time has passed
                                    if error_lower.contains("submission window closed") ||
                                       error_lower.contains("window closed") {
                                        continue;
                                    }

                                    // Don't retry if solution doesn't meet difficulty (invalid nonce)
                                    if error_lower.contains("does not meet difficulty") ||
                                       error_lower.contains("difficulty") && error_lower.contains("not meet") {
                                        continue;
                                    }
                                }

                                failed_solutions.push(record);
                            }
                        }
                    }
                }
            }
        }
    }

    failed_solutions
}

/// Load difficult tasks from file
fn load_difficult_tasks() -> Vec<DifficultTask> {
    if !Path::new(DIFFICULT_TASKS_FILE).exists() {
        return Vec::new();
    }

    match fs::read_to_string(DIFFICULT_TASKS_FILE) {
        Ok(content) => {
            serde_json::from_str::<Vec<DifficultTask>>(&content).unwrap_or_else(|_| Vec::new())
        }
        Err(_) => Vec::new(),
    }
}

/// Save difficult tasks to file
fn save_difficult_task(task: DifficultTask) -> Result<(), Box<dyn std::error::Error>> {
    let mut tasks = load_difficult_tasks();

    // Check if already exists (update if found)
    let exists = tasks.iter_mut().find(|t| {
        t.wallet_address == task.wallet_address && t.challenge_id == task.challenge_id
    });

    if let Some(existing) = exists {
        *existing = task;
    } else {
        tasks.push(task);
    }

    let json = serde_json::to_string_pretty(&tasks)?;
    fs::write(DIFFICULT_TASKS_FILE, json)?;
    Ok(())
}

/// Check if task is marked as difficult
fn is_difficult_task(wallet_address: &str, challenge_id: &str, difficult_tasks: &[DifficultTask]) -> bool {
    difficult_tasks.iter().any(|t| {
        t.wallet_address == wallet_address && t.challenge_id == challenge_id
    })
}

/// Build cached preimage suffix (everything after nonce)
/// This is computed once before mining to avoid repeated allocations
fn build_preimage_suffix(address: &str, challenge: &Challenge) -> Vec<u8> {
    let mut suffix = Vec::new();
    suffix.extend_from_slice(address.as_bytes());
    suffix.extend_from_slice(challenge.challenge_id.as_bytes());
    suffix.extend_from_slice(challenge.difficulty.as_bytes());
    suffix.extend_from_slice(challenge.no_pre_mine.as_bytes());
    suffix.extend_from_slice(challenge.latest_submission.as_bytes());
    suffix.extend_from_slice(challenge.no_pre_mine_hour.as_bytes());
    suffix
}

/// Optimized construct_preimage using pre-cached suffix
/// Reduces from 7 extend_from_slice calls to just 2 per nonce
/// Uses write! to avoid intermediate String allocation from format!
#[inline(always)]
fn construct_preimage_fast(nonce: u64, suffix: &[u8]) -> Vec<u8> {
    use std::io::Write;

    let mut preimage = Vec::with_capacity(16 + suffix.len());
    write!(&mut preimage, "{:016x}", nonce).unwrap();
    preimage.extend_from_slice(suffix);
    preimage
}

/// Fetch current challenge from Scavenger Mine API
fn fetch_current_challenge() -> Result<Challenge, Box<dyn std::error::Error>> {
    let url = format!("{}/challenge", SCAVENGER_API_BASE);
    let response = reqwest::blocking::get(&url)?;
    let data: ChallengeResponse = response.json()?;
    Ok(data.challenge)
}

/// Update and filter active challenges list
/// Adds new challenge if not present, removes expired challenges, and sorts by difficulty
fn update_active_challenges(
    challenges_cache: &mut Vec<Challenge>,
    num_threads: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch current challenge from API
    let current_challenge = fetch_current_challenge()?;

    // Add to cache if not already present (check by challenge_id)
    let already_exists = challenges_cache.iter().any(|c| c.challenge_id == current_challenge.challenge_id);
    if !already_exists {
        log_mining_progress(&format!("📥 New challenge discovered: {}", current_challenge.challenge_id));
        challenges_cache.push(current_challenge);
    }

    // Filter out inactive challenges (where deadline is within 1 hour or already passed)
    let initial_count = challenges_cache.len();
    challenges_cache.retain(|c| {
        let is_active = c.is_active();
        if !is_active {
            log_mining_progress(&format!("⏰ Challenge {} expires soon (< 1 hour), removing from active list", c.challenge_id));
        }
        is_active
    });
    let removed_count = initial_count - challenges_cache.len();
    if removed_count > 0 {
        log_mining_progress(&format!("🗑️  Removed {} challenge(s) expiring within 1 hour", removed_count));
    }

    // Sort using comprehensive comparison:
    // 1. Total zero bits (fewer = easier, zeros are constraints)
    // 2. Leading zero bits (more = easier, consecutive pattern at start)
    // 3. Latest submission (thread-count dependent):
    //    - < 6 threads: newer first (faster refresh strategy)
    //    - >= 6 threads: older first (less competition strategy)
    // 4. Challenge ID (deterministic tiebreaker)
    challenges_cache.sort_by(|a, b| a.compare_for_selection(b, num_threads));

    Ok(())
}

/// Check if challenge is still open by fetching current challenge
/// A challenge is open if it's still active (current time < latest_submission)
fn is_challenge_still_open(solution: &SolutionRecord) -> bool {
    // Try to fetch the current challenge to see if it matches
    match fetch_current_challenge() {
        Ok(current_challenge) => {
            // If it's the same challenge and still active, it's open
            if current_challenge.challenge_id == solution.challenge_id {
                return current_challenge.is_active();
            }
            // If it's a different challenge, the old one is likely expired
            false
        }
        Err(_) => {
            // If we can't fetch, assume it might still be open (network issue)
            true
        }
    }
}

/// Check if a solution already exists for a wallet-challenge pair
fn solution_exists(wallet_address: &str, challenge_id: &str) -> bool {
    let clean_challenge_id = challenge_id.replace("*", "").replace("/", "_");
    let filename = format!("{}/{}_{}.json", SOLUTIONS_DIR, wallet_address, clean_challenge_id);

    Path::new(&filename).exists()
}

/// Select the best challenge for a wallet (easiest unsolved challenge)
fn select_challenge_for_wallet(wallet_address: &str, challenges: &[Challenge]) -> Option<Challenge> {
    // Iterate through challenges (already sorted by difficulty, easiest first)
    // This maximizes solutions/hour by solving easy challenges quickly
    for challenge in challenges {
        if !solution_exists(wallet_address, &challenge.challenge_id) {
            return Some(challenge.clone());
        }
    }

    // If all challenges have been solved, return None
    None
}

/// Result of Scavenger Mine submission
#[derive(Debug)]
enum SubmitResult {
    Success(CryptoReceipt),
    Failed(String), // Error message
}

/// Submit nonce to Scavenger Mine API
fn submit_to_scavenger(
    wallet_address: &str,
    challenge_id: &str,
    nonce: u64,
) -> Result<SubmitResult, Box<dyn std::error::Error>> {
    let url = format!("{}/solution/{}/{}/{:016x}",
                     SCAVENGER_API_BASE, wallet_address, challenge_id, nonce);

    let client = reqwest::blocking::Client::builder()
        .gzip(true)
        .build()?;

    let response = client.post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "en-US,en;q=0.9")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Connection", "keep-alive")
        .json(&serde_json::json!({}))
        .send()?;

    let status = response.status();

    // Check for success (200-299) or specifically 201 Created
    if status.is_success() || status.as_u16() == 201 {
        // Try to parse the response
        match response.json::<ScavengerSubmitResponse>() {
            Ok(result) => {
                if let Some(receipt) = result.crypto_receipt {
                    Ok(SubmitResult::Success(receipt))
                } else {
                    let error_msg = "API returned success but no crypto_receipt".to_string();
                    log_mining_progress(&format!("⚠️  {}", error_msg));
                    Ok(SubmitResult::Failed(error_msg))
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to parse response: {}", e);
                log_mining_progress(&format!("⚠️  {}", error_msg));
                Ok(SubmitResult::Failed(error_msg))
            }
        }
    } else {
        // Get response text for error logging
        let error_text = response.text().unwrap_or_else(|_| "Unable to read response".to_string());
        let error_msg = format!("HTTP {}: {}", status.as_u16(), error_text);
        log_mining_progress(&format!("❌ Scavenger API error: {}", error_msg));
        Ok(SubmitResult::Failed(error_msg))
    }
}

/// Load user wallets from file
fn load_user_wallets(path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if !Path::new(path).exists() {
        return Err(format!("Wallets file not found: {}", path).into());
    }

    let content = fs::read_to_string(path)?;
    let wallets: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect();

    if wallets.is_empty() {
        return Err("No valid wallet addresses found in file".into());
    }

    Ok(wallets)
}

/// Result of mining operation
enum MiningResult {
    Found(u64),              // Solution found with nonce
    TooHard(u64, u64),       // Exceeded threshold: (total_hashes, duration_secs)
    NotFound,                // No solution found
}

/// Mine a single solution using Rayon for optimal CPU utilization
fn mine_single_solution(
    rom: Arc<Rom>,
    address: &str,
    challenge: &Challenge,
    num_threads: usize,
    max_hashes: Option<u64>,
) -> MiningResult {
    // Use atomic counter to track thread indices reliably (thread name parsing may fail)
    let thread_counter = Arc::new(AtomicU64::new(0));

    // Decode difficulty once before mining (optimization - avoids repeated hex decoding in hot loop)
    let diff_bytes = match hex::decode(&challenge.difficulty) {
        Ok(bytes) => bytes,
        Err(_) => {
            log_mining_progress(&format!("❌ Invalid difficulty hex string: {}", challenge.difficulty));
            return MiningResult::NotFound;
        }
    };

    // Build preimage suffix once (optimization - avoids 6 extend_from_slice calls per nonce)
    let preimage_suffix = build_preimage_suffix(address, challenge);
    let preimage_suffix = Arc::new(preimage_suffix);

    // Configure rayon thread pool to use exact number of threads with processor group affinity
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .spawn_handler({
            let counter = thread_counter.clone();
            move |thread| {
                // Atomically get the next thread index
                #[allow(unused_variables)]  // Used on Windows for thread affinity
                let thread_idx = counter.fetch_add(1, Ordering::SeqCst) as usize;

                let mut b = std::thread::Builder::new();
                if let Some(name) = thread.name() {
                    b = b.name(name.to_owned());
                }
                if let Some(stack_size) = thread.stack_size() {
                    b = b.stack_size(stack_size);
                }
                b.spawn(move || {
                    // Set processor group affinity on Windows for >64 logical processors
                    #[cfg(windows)]
                    {
                        set_thread_processor_group_affinity(thread_idx);
                    }
                    thread.run()
                })?;
                Ok(())
            }
        })
        .build()
        .unwrap();

    let found = Arc::new(AtomicBool::new(false));
    let hash_count = Arc::new(AtomicU64::new(0));
    let result: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));

    // Strided approach: each thread gets start_nonce = thread_id, stride = num_threads
    // Thread 0: 0, 4, 8, 12, ...
    // Thread 1: 1, 5, 9, 13, ...
    // Thread 2: 2, 6, 10, 14, ...
    // Thread 3: 3, 7, 11, 15, ...
    // This provides better load balancing and lower variance than range partitioning
    let stride = num_threads as u64;
    let work_assignments: Vec<(u64, usize)> = (0..num_threads)
        .map(|thread_id| {
            let start_nonce = thread_id as u64;
            (start_nonce, thread_id)
        })
        .collect();

    let start_time = Instant::now();
    let last_log_time = Arc::new(Mutex::new(Instant::now()));

    // Use rayon's parallel iterator for better CPU saturation
    pool.install(|| {
        work_assignments.par_iter().for_each(|(start_nonce, thread_id)| {
            let mut nonce = *start_nonce;
            let mut local_count = 0u64;
            let suffix = Arc::clone(&preimage_suffix);

            // Each thread increments by stride for interleaved nonce testing
            loop {
                if found.load(Ordering::Relaxed) {
                    break;
                }

                let preimage = construct_preimage_fast(nonce, &suffix);
                let result_hash = hash(&preimage, &rom, NB_LOOPS, NB_INSTRS);

                hash_count.fetch_add(1, Ordering::Relaxed);
                local_count += 1;

                if check_difficulty(&result_hash, &diff_bytes) {
                    found.store(true, Ordering::Relaxed);
                    log_mining_progress(&format!("🎉 [Thread {}] Found solution! Nonce: {:016x}", thread_id, nonce));

                    let mut res = result.lock().unwrap();
                    *res = Some(nonce);
                    return;
                }

                // Strided increment (wraps on overflow, but impossible in practice)
                nonce += stride;

                if local_count % 5000 == 0 {
                    // Log progress and check hash limit every 30 seconds
                    let mut last_log = last_log_time.lock().unwrap();
                    if last_log.elapsed() >= Duration::from_secs(30) {
                        // Load total hash count once and reuse
                        let total = hash_count.load(Ordering::Relaxed);
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let hash_rate = if elapsed > 0.0 { total as f64 / elapsed } else { 0.0 };
                        log_mining_progress(&format!(
                            "⛏️  Mining... {} total hashes ({:.2} H/s overall)",
                            total, hash_rate
                        ));
                        *last_log = Instant::now();

                        // Check hash limit (if set) - this is a soft limit
                        if let Some(max_h) = max_hashes {
                            if total >= max_h {
                                found.store(true, Ordering::Relaxed);
                                log_mining_progress(&format!("⏱️  Hash limit reached: {} hashes", total));
                                return;
                            }
                        }
                    }
                }
            }
        });
    });

    let res = result.lock().unwrap();
    let total_hashes = hash_count.load(Ordering::Relaxed);
    let duration_secs = start_time.elapsed().as_secs();

    match *res {
        Some(nonce) => MiningResult::Found(nonce),
        None => {
            // Check if we hit the hash limit (soft limit, may be slightly exceeded)
            if let Some(max_h) = max_hashes {
                if total_hashes >= max_h {
                    return MiningResult::TooHard(total_hashes, duration_secs);
                }
            }
            MiningResult::NotFound
        }
    }
}

/// Check and retry failed submissions (called in main mining loop)
/// Only retries if at least 1 hour has passed since last retry
fn check_and_retry_failed_submissions() {
    let failed_solutions = get_failed_solutions();

    if failed_solutions.is_empty() {
        return;
    }

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut retried_count = 0;

    for mut solution in failed_solutions {
        // Check if at least 1 hour has passed since last retry
        let should_retry = if let Some(ref last_retry) = solution.last_retry_at {
            // Parse last retry timestamp
            if let Ok(last_time) = chrono::DateTime::parse_from_rfc3339(last_retry) {
                let last_timestamp = last_time.timestamp() as u64;
                let elapsed = current_time.saturating_sub(last_timestamp);
                elapsed >= 3600 // 1 hour in seconds
            } else {
                true // If can't parse, retry
            }
        } else {
            // Never retried before, check time since found
            if let Ok(found_time) = chrono::DateTime::parse_from_rfc3339(&solution.found_at) {
                let found_timestamp = found_time.timestamp() as u64;
                let elapsed = current_time.saturating_sub(found_timestamp);
                elapsed >= 3600 // 1 hour since found
            } else {
                true // If can't parse, retry
            }
        };

        if !should_retry {
            continue;
        }

        // Check if challenge is still open
        if !is_challenge_still_open(&solution) {
            log_mining_progress(&format!("⏭️  Challenge {} no longer active", solution.challenge_id));
            solution.status = "challenge_closed".to_string();
            solution.error_message = Some("Challenge no longer in active list".to_string());
            if let Err(e) = update_solution_record(&solution) {
                log_mining_progress(&format!("⚠️  Failed to update solution record: {}", e));
            }
            continue;
        }

        // Check if already too many retries
        if solution.retry_count >= 10 {
            if solution.status != "abandoned" {
                solution.status = "abandoned".to_string();
                if let Err(e) = update_solution_record(&solution) {
                    log_mining_progress(&format!("⚠️  Failed to update solution record: {}", e));
                }
            }
            continue;
        }

        log_mining_progress(&format!("🔁 Retrying solution: {}... (attempt #{})",
            &solution.challenge_id[..16.min(solution.challenge_id.len())],
            solution.retry_count + 1));

        // Parse nonce from hex string
        let nonce = match u64::from_str_radix(&solution.nonce, 16) {
            Ok(n) => n,
            Err(e) => {
                log_mining_progress(&format!("❌ Invalid nonce format: {}", e));
                continue;
            }
        };

        // Attempt resubmission
        match submit_to_scavenger(&solution.wallet_address, &solution.challenge_id, nonce) {
            Ok(SubmitResult::Success(crypto_receipt)) => {
                log_mining_progress("   ✅ Retry successful!");

                solution.status = "submitted".to_string();
                solution.crypto_receipt = Some(crypto_receipt);
                solution.submitted_at = Some(get_timestamp());
                solution.error_message = None;
                solution.retry_count += 1;
                solution.last_retry_at = Some(get_timestamp());

                if let Err(e) = update_solution_record(&solution) {
                    log_mining_progress(&format!("⚠️  Failed to update solution record: {}", e));
                }

                retried_count += 1;
            }
            Ok(SubmitResult::Failed(error_msg)) => {
                log_mining_progress(&format!("   ❌ Retry failed: {}", error_msg));

                // Check if this is a non-retriable error
                let error_lower = error_msg.to_lowercase();
                if error_lower.contains("solution already exists") ||
                   error_lower.contains("already exists") {
                    solution.status = "duplicate".to_string();
                    solution.error_message = Some(error_msg);
                    log_mining_progress("   ⏭️  Marked as duplicate (won't retry)");
                } else if error_lower.contains("does not meet difficulty") ||
                          (error_lower.contains("difficulty") && error_lower.contains("not meet")) {
                    solution.status = "invalid_nonce".to_string();
                    solution.error_message = Some(error_msg);
                    log_mining_progress("   ⏭️  Marked as invalid (won't retry)");
                } else {
                    solution.retry_count += 1;
                    solution.last_retry_at = Some(get_timestamp());
                    solution.error_message = Some(error_msg);

                    if solution.retry_count >= 10 {
                        solution.status = "abandoned".to_string();
                        log_mining_progress(&format!("   ⚠️  Giving up after {} attempts", solution.retry_count));
                    }
                }

                if let Err(e) = update_solution_record(&solution) {
                    log_mining_progress(&format!("⚠️  Failed to update solution record: {}", e));
                }

                retried_count += 1;
            }
            Err(e) => {
                log_mining_progress(&format!("   ❌ Network error: {}", e));

                solution.retry_count += 1;
                solution.last_retry_at = Some(get_timestamp());
                solution.error_message = Some(format!("Network error: {}", e));

                if let Err(e) = update_solution_record(&solution) {
                    log_mining_progress(&format!("⚠️  Failed to update solution record: {}", e));
                }

                retried_count += 1;
            }
        }

        // Small delay between retries
        if retried_count < get_failed_solutions().len() {
            thread::sleep(Duration::from_millis(500));
        }
    }

    if retried_count > 0 {
        log_mining_progress(&format!("✓ Processed {} resubmission(s)", retried_count));
    }
}

/// Get user input from stdin
fn get_user_input(prompt: &str, default: &str) -> String {
    print!("{} [default: {}]: ", prompt, default);
    std::io::stdout().flush().unwrap();

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();

    if input.is_empty() {
        default.to_string()
    } else {
        input.to_string()
    }
}

/// Parse configuration from either CLI args or interactive prompts
fn get_configuration() -> (String, f64, Option<f64>) {
    let args: Vec<String> = env::args().collect();

    // Check if running in CLI mode (has arguments)
    if args.len() > 1 {
        // CLI mode - parse arguments
        let wallets_file = args.get(1)
            .map(|s| s.as_str())
            .unwrap_or("wallets.txt");

        let cpu_usage = args.get(2)
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(50.0)  // Default to 50% CPU usage for maximum performance
            .min(100.0)
            .max(1.0);

        let max_hashes_millions = args.get(3)
            .and_then(|s| s.parse::<f64>().ok());

        (wallets_file.to_string(), cpu_usage, max_hashes_millions)
    } else {
        // Interactive mode - prompt user
        println!("\n📝 Configuration Setup (press Enter to use defaults)\n");

        // Get wallets file location
        let wallets_file = get_user_input("📂 Wallets file location", "wallets.txt");

        // Get CPU usage percentage
        let cpu_input = get_user_input("💻 Maximum CPU usage (25/50/75/100)", "50");
        let cpu_usage = cpu_input.parse::<f64>()
            .unwrap_or(50.0)  // Default to 50% CPU usage for maximum performance
            .min(100.0)
            .max(1.0);

        // Get max hashes threshold (optional)
        println!("\n⏱️  Maximum hashes per task (auto-skip if exceeded)?");
        println!("   Default: mine until solution found (no limit)");
        println!("   Examples: 100 = 100M hashes, 0.5 = 500K hashes");
        let max_hashes_input = get_user_input("🔢 Max hashes in millions (press Enter for no limit)", "none");
        let max_hashes_millions = if max_hashes_input.is_empty() || max_hashes_input == "none" {
            None
        } else {
            max_hashes_input.parse::<f64>().ok()
        };

        println!();

        (wallets_file, cpu_usage, max_hashes_millions)
    }
}

fn main() {
    println!("╔═══════════════════════════════════════════════════╗");
    println!("║   Scavenger Mine USER-ONLY Miner v4.0             ║");
    println!("║   - No profit sharing (100% for your wallets)    ║");
    println!("║   - Dual core support                            ║");
    println!("║   - Optimize hash rate                           ║");
    println!("║   - Auto skip difficult challenges               ║");
    println!("║   - Auto select easiest challenge to solve       ║");
    println!("╚═══════════════════════════════════════════════════╝\n");

    // Setup directories
    if let Err(e) = setup_directories() {
        eprintln!("Failed to create output directories: {}", e);
        std::process::exit(1);
    }

    log_mining_progress("🚀 Starting USER-ONLY Miner (No Profit Sharing)");
    log_mining_progress(&format!("📁 Solutions will be saved to: {}/", SOLUTIONS_DIR));
    log_mining_progress(&format!("📋 Logs will be saved to: {}/", LOGS_DIR));

    // Get configuration (either from CLI args or interactive prompts)
    let (wallets_file, cpu_usage, max_hashes_millions) = get_configuration();

    // Calculate hash threshold (if provided, convert millions to actual count)
    let max_hashes = max_hashes_millions.map(|m| (m * 1_000_000.0) as u64);

    let config_msg = match max_hashes_millions {
        Some(hashes) => format!(
            "⚙️  Configuration: Wallets file: {}, CPU usage: {}%, Max hashes: {}M",
            wallets_file, cpu_usage, hashes
        ),
        None => format!(
            "⚙️  Configuration: Wallets file: {}, CPU usage: {}%, No limit",
            wallets_file, cpu_usage
        ),
    };
    log_mining_progress(&config_msg);

    // Load difficult tasks
    let difficult_tasks = load_difficult_tasks();
    if !difficult_tasks.is_empty() {
        log_mining_progress(&format!("📋 Loaded {} difficult task(s) to skip", difficult_tasks.len()));
    }

    // Load user wallets
    let user_wallets = match load_user_wallets(&wallets_file) {
        Ok(wallets) => {
            log_mining_progress(&format!("✅ Loaded {} user wallet(s)", wallets.len()));
            wallets
        }
        Err(e) => {
            log_mining_progress(&format!("❌ Error loading wallets: {}", e));
            eprintln!("\n❌ ERROR: Could not load wallets file '{}'", wallets_file);
            eprintln!("\n📝 Please create this file with one wallet address per line");
            eprintln!("   Example content:");
            eprintln!("   addr1q8upjxynn626c772r5nzym...");
            eprintln!("   addr1qpxvug56xgecxhuzv3c60u4...");
            eprintln!("\n💡 Tip: The file should be in the same folder as this executable");
            eprintln!("   Current folder: {}", env::current_dir().unwrap().display());
            eprintln!("\nPress Enter to exit...");

            // Wait for user to acknowledge in interactive mode
            let args: Vec<String> = env::args().collect();
            if args.len() == 1 {
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap();
            }

            std::process::exit(1);
        }
    };

    // Generate miner ID
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    let miner_id = format!("user-only-miner-{}-{}", hostname,
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());

    log_mining_progress(&format!("🆔 Miner ID: {}", miner_id));

    // Calculate number of threads - use Windows processor group aware detection for systems with >64 logical processors
    let total_cpus = get_total_logical_processors();
    let physical_cores = num_cpus::get_physical();
    let num_threads = ((total_cpus as f64 * cpu_usage / 100.0).ceil() as usize).max(1);

    // Log detailed CPU information
    if physical_cores < total_cpus {
        log_mining_progress(&format!(
            "💻 System: {} logical processors ({} physical cores with hyper-threading), using {} threads ({}%)",
            total_cpus, physical_cores, num_threads, cpu_usage
        ));
        log_mining_progress(&format!(
            "   ℹ️  Hyper-threading detected: {} threads per core",
            total_cpus / physical_cores
        ));
    } else {
        log_mining_progress(&format!(
            "💻 System: {} CPU cores, using {} threads ({}%)",
            total_cpus, num_threads, cpu_usage
        ));
    }

    // Additional tip for users with hyper-threading
    if num_threads >= total_cpus && physical_cores < total_cpus {
        log_mining_progress("   ✅ Using all logical processors including hyper-threads for maximum performance");
    }

    // ROM cache
    let mut rom_cache = RomCache::new();

    // Statistics
    let mut total_solutions = 0u64;
    let mut current_wallet_index = 0usize;
    let session_start = Instant::now();

    // Challenges cache (fetch once per cycle or when needed)
    let mut challenges_cache: Vec<Challenge> = vec![];
    let mut last_challenges_fetch = Instant::now();

    // Main mining loop - USER ONLY MODE
    loop {
        // Update active challenges periodically (every cycle or every 5 minutes)
        // This fetches the current challenge, adds it to cache, and removes expired ones
        if challenges_cache.is_empty() || last_challenges_fetch.elapsed() > Duration::from_secs(300) {
            match update_active_challenges(&mut challenges_cache, num_threads) {
                Ok(()) => {
                    last_challenges_fetch = Instant::now();
                    log_mining_progress(&format!("📥 Active challenges: {} (sorted by difficulty, easiest first)", challenges_cache.len()));
                }
                Err(e) => {
                    log_mining_progress(&format!("⚠️  Error updating challenges: {}, will retry later", e));
                    if challenges_cache.is_empty() {
                        thread::sleep(Duration::from_secs(30));
                        continue;
                    }
                }
            }
        }

        // Mine for user - cycle through user wallets
        let user_wallet = &user_wallets[current_wallet_index];
        current_wallet_index = (current_wallet_index + 1) % user_wallets.len();

        log_mining_progress(&format!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"));
        log_mining_progress(&format!("👤 Mining for USER (Solution #{})", total_solutions + 1));
        log_mining_progress(&format!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"));

        // Select best challenge for this wallet (easiest unsolved challenge)
        let challenge = match select_challenge_for_wallet(user_wallet, &challenges_cache) {
            Some(challenge) => challenge,
            None => {
                log_mining_progress(&format!("✅ All active challenges solved for wallet: {}...", &user_wallet[..20.min(user_wallet.len())]));
                log_mining_progress("📥 Updating challenges list...");

                // Force refresh challenges
                match update_active_challenges(&mut challenges_cache, num_threads) {
                    Ok(()) => {
                        last_challenges_fetch = Instant::now();
                        log_mining_progress(&format!("📥 Active challenges updated: {}", challenges_cache.len()));
                    }
                    Err(e) => {
                        log_mining_progress(&format!("❌ Error updating challenges: {}", e));
                        thread::sleep(Duration::from_secs(30));
                        continue;
                    }
                }

                // Try again with updated challenges
                match select_challenge_for_wallet(user_wallet, &challenges_cache) {
                    Some(challenge) => challenge,
                    None => {
                        log_mining_progress("⚠️  No available challenges to mine, waiting...");
                        thread::sleep(Duration::from_secs(60));
                        continue;
                    }
                }
            }
        };

        log_mining_progress(&format!("📋 Challenge: {}", challenge.challenge_id));
        log_mining_progress(&format!("👛 Wallet: {}...", &user_wallet[..20.min(user_wallet.len())]));
        log_mining_progress(&format!("🎯 Difficulty: {}", challenge.difficulty));

        // Check if this task is marked as too difficult
        if is_difficult_task(user_wallet, &challenge.challenge_id, &difficult_tasks) {
            log_mining_progress("⏭️  Skipping: Task marked as too difficult");
            continue;
        }

        let rom = rom_cache.get_or_create(&challenge.no_pre_mine);

        log_mining_progress("⛏️  Starting mining threads...");
        let start_time = Instant::now();
        match mine_single_solution(rom, user_wallet, &challenge, num_threads, max_hashes) {
            MiningResult::Found(nonce) => {
                let elapsed = start_time.elapsed();
                log_mining_progress(&format!("✅ Solution found in {:.2?}", elapsed));

                let found_timestamp = get_timestamp();

                match submit_to_scavenger(user_wallet, &challenge.challenge_id, nonce) {
                    Ok(SubmitResult::Success(crypto_receipt)) => {
                        log_mining_progress("✅ Submitted to Scavenger Mine");

                        // Export solution with crypto receipt
                        let record = SolutionRecord {
                            wallet_address: user_wallet.clone(),
                            challenge_id: challenge.challenge_id.clone(),
                            nonce: format!("{:016x}", nonce),
                            found_at: found_timestamp,
                            submitted_at: Some(get_timestamp()),
                            crypto_receipt: Some(crypto_receipt),
                            status: "submitted".to_string(),
                            error_message: None,
                            retry_count: 0,
                            last_retry_at: None,
                        };

                        if let Err(e) = export_solution(&record) {
                            log_mining_progress(&format!("⚠️  Failed to export solution: {}", e));
                        }

                        total_solutions += 1;
                    }
                    Ok(SubmitResult::Failed(error_msg)) => {
                        log_mining_progress(&format!("❌ Scavenger submission failed: {}", error_msg));

                        // Check if this is a non-retriable error
                        let error_lower = error_msg.to_lowercase();
                        let status = if error_lower.contains("solution already exists") ||
                                        error_lower.contains("already exists") {
                            log_mining_progress("   ℹ️  Solution already submitted elsewhere (won't retry)");
                            "duplicate".to_string()
                        } else if error_lower.contains("does not meet difficulty") ||
                                  (error_lower.contains("difficulty") && error_lower.contains("not meet")) {
                            log_mining_progress("   ℹ️  Invalid nonce (won't retry)");
                            "invalid_nonce".to_string()
                        } else {
                            log_mining_progress("   🔄 Will retry after 1 hour");
                            "failed".to_string()
                        };

                        // Export solution with error
                        let record = SolutionRecord {
                            wallet_address: user_wallet.clone(),
                            challenge_id: challenge.challenge_id.clone(),
                            nonce: format!("{:016x}", nonce),
                            found_at: found_timestamp,
                            submitted_at: Some(get_timestamp()),
                            crypto_receipt: None,
                            status,
                            error_message: Some(error_msg),
                            retry_count: 0,
                            last_retry_at: None,
                        };

                        if let Err(e) = export_solution(&record) {
                            log_mining_progress(&format!("⚠️  Failed to export solution: {}", e));
                        }
                    }
                    Err(e) => {
                        log_mining_progress(&format!("❌ Network error submitting to Scavenger: {}", e));
                        log_mining_progress("   🔄 Will retry after 1 hour");

                        // Export solution with error - will be retried
                        let record = SolutionRecord {
                            wallet_address: user_wallet.clone(),
                            challenge_id: challenge.challenge_id.clone(),
                            nonce: format!("{:016x}", nonce),
                            found_at: found_timestamp,
                            submitted_at: None,
                            crypto_receipt: None,
                            status: "error: network".to_string(),
                            error_message: Some(format!("Network error: {}", e)),
                            retry_count: 0,
                            last_retry_at: None,
                        };

                        if let Err(e) = export_solution(&record) {
                            log_mining_progress(&format!("⚠️  Failed to export solution: {}", e));
                        }
                    }
                }
            }
            MiningResult::TooHard(hashes, duration) => {
                log_mining_progress(&format!("⏭️  Task too difficult: {} hashes in {}s", hashes, duration));
                let difficult = DifficultTask {
                    wallet_address: user_wallet.clone(),
                    challenge_id: challenge.challenge_id.clone(),
                    marked_at: get_timestamp(),
                    total_hashes: hashes,
                    mining_duration_secs: duration,
                };
                if let Err(e) = save_difficult_task(difficult) {
                    log_mining_progress(&format!("⚠️  Failed to save difficult task: {}", e));
                }
            }
            MiningResult::NotFound => {
                log_mining_progress("❌ No solution found");
            }
        }

        // Check and retry any failed submissions (only if at least 1 hour has passed)
        check_and_retry_failed_submissions();

        // Print statistics
        println!("\n📊 Session Statistics:");
        println!("   Total solutions: {} (100% for your wallets)", total_solutions);
        println!("   Runtime: {:.2?}", session_start.elapsed());

        // Calculate and display average time per solution
        if total_solutions > 0 {
            let avg_time_secs = session_start.elapsed().as_secs_f64() / total_solutions as f64;
            let avg_minutes = (avg_time_secs / 60.0).floor() as u64;
            let avg_seconds = (avg_time_secs % 60.0).floor() as u64;
            println!("   Average time per solution: {}m {}s\n", avg_minutes, avg_seconds);
        } else {
            println!();
        }

        thread::sleep(Duration::from_secs(2));
    }
}
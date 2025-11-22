# Free Scavenger Miner

A high-performance, user-only cryptocurrency miner for the [Defensio Scavenger Mine](https://defensio.io/mine) mining game built with Rust.

## Overview

Free Scavenger Miner is a standalone mining application that uses the AshMaize proof-of-work algorithm to solve Scavenger Mine challenges. Unlike pool miners, this miner keeps 100% of rewards for your wallets with no profit sharing.

### Key Features

- **100% User Profit** - No profit sharing, all rewards go to your configured wallets
- **Multi-Core CPU Support** - Efficient parallel mining using all available CPU cores
- **Windows Processor Group Awareness** - Full support for systems with 64+ logical processors and multi-socket configurations
- **Smart Challenge Selection** - Automatically selects the easiest available challenges to maximize solutions per hour
- **Auto-Skip Difficult Tasks** - Configurable hash threshold to skip extremely difficult challenges
- **Solution Export & Retry** - Automatic solution persistence and intelligent retry logic for failed submissions
- **Cross-Platform** - Builds on Windows, Linux, and macOS

## Requirements

### Running the Miner (Prebuilt Binary)

- **CPU** - Any modern CPU (more cores = better performance)
- **RAM** - Minimum 2GB available (ROM initialization requires ~1GB)
- **Network** - Internet connection for API communication

### Building from Source

- **Rust** 1.75.0 or later ([Install Rust](https://rustup.rs/))
- **Platform-Specific Build Tools:**
  - **Windows:** Visual Studio Build Tools or MinGW-w64
  - **Linux:** GCC/build-essential or musl-tools (for static binaries)
  - **macOS:** Xcode Command Line Tools

## Download Prebuilt Executable

**Don't want to build from source?** Download the latest prebuilt executable for your platform:

### [📥 Download from Releases](https://github.com/danny-nguyen-2702/Free-Scavenger-Miner/releases)

Available platforms:
- **Windows** (x64) - `scavenger-miner.exe`
- **Linux** (x64, static) - `scavenger-miner`

### Quick Setup (Prebuilt Binary)

1. **Download** the executable for your platform from the [Releases page](https://github.com/danny-nguyen-2702/Free-Scavenger-Miner/releases)
2. **Extract** the archive (if compressed)
3. **Create** a `wallets.txt` file in the same directory with your wallet addresses
4. **Run** the executable:
   - **Windows**: Double-click `scavenger-miner.exe` or run from command prompt
   - **Linux/macOS**: `chmod +x scavenger-miner && ./scavenger-miner`

That's it! No compilation needed.

---

## Build from Source

If you prefer to build from source or need a custom configuration:

### 1. Clone the Repository

```bash
git clone https://github.com/danny-nguyen-2702/Free-Scavenger-Miner.git
cd Free-Scavenger-Miner/scavenger-miner-code
```

### 2. Create Your Wallets File

Create a `wallets.txt` file with your Cardano wallet addresses (one per line):

```
addr1q8upjxynn626c772r5nzym...
addr1qpxvug56xgecxhuzv3c60u4...
```

### 3. Build the Miner

**Quick build (all platforms):**
```bash
cargo build --bin scavenger-miner --release
```

**For detailed platform-specific build instructions, see [BUILD_GUIDE.md](BUILD_GUIDE.md)**

### 4. Run the Miner

**Interactive mode:**
```bash
./target/release/scavenger-miner
```

**CLI mode with arguments:**
```bash
./target/release/scavenger-miner wallets.txt 50 100
```

Arguments:
- `wallets.txt` - Path to wallet addresses file
- `50` - CPU usage percentage (1-100)
- `100` - Max hashes in millions before auto-skip (optional)

## Configuration

### Interactive Mode

When running without arguments, the miner prompts for configuration:

```
📂 Wallets file location [default: wallets.txt]:
💻 Maximum CPU usage (25/50/75/100) [default: 50]:
🔢 Max hashes in millions (press Enter for no limit) [default: none]:
```

### CLI Mode

```bash
scavenger-miner <wallets_file> <cpu_usage> [max_hashes_millions]
```

**Examples:**

```bash
# Use wallets.txt, 75% CPU, no hash limit
./target/release/scavenger-miner wallets.txt 75

# Use my-wallets.txt, 100% CPU, skip after 500M hashes
./target/release/scavenger-miner my-wallets.txt 100 500

# Use wallets.txt, 25% CPU (low power), skip after 50M hashes
./target/release/scavenger-miner wallets.txt 25 50
```

### CPU Usage Guidelines

| Usage | Description | Best For |
|-------|-------------|----------|
| 25% | Low power consumption | Background mining on laptops |
| 50% | Balanced performance | General purpose mining |
| 75% | High performance | Dedicated mining systems |
| 100% | Maximum performance | All-out mining |

## Output & Logs

The miner creates two directories for output:

### `solutions/`
Contains JSON files for each discovered solution:

```json
{
  "wallet_address": "addr1q8upjxynn...",
  "challenge_id": "challenge_123",
  "nonce": "0000000012abcdef",
  "found_at": "2025-01-15T10:30:45Z",
  "submitted_at": "2025-01-15T10:30:46Z",
  "crypto_receipt": {
    "preimage": "...",
    "timestamp": "...",
    "signature": "..."
  },
  "status": "submitted"
}
```

### `logs/`
Contains timestamped mining logs (`mining.log`):

```
[2025-01-15T10:30:00Z] 🚀 Starting USER-ONLY Miner
[2025-01-15T10:30:01Z] ✅ Loaded 2 user wallet(s)
[2025-01-15T10:30:15Z] ⛏️  Mining... 1000000 total hashes (50000 H/s)
[2025-01-15T10:32:30Z] 🎉 Found solution! Nonce: 0000000012abcdef
```

## Project Structure

```
Free-Scavenger-Miner/
├── scavenger-miner-code/     # Main mining application
│   ├── src/
│   │   └── main.rs           # Miner implementation
│   ├── Cargo.toml            # Rust dependencies
│   └── wallets.txt           # Your wallet addresses (create this)
├── ce-ashmaize/              # AshMaize PoW library
│   ├── src/
│   ├── benches/
│   └── crates/
├── BUILD_GUIDE.md            # Detailed build instructions
└── README.md                 # This file
```

## How It Works

1. **Challenge Fetching** - Retrieves active challenges from Scavenger Mine API
2. **Smart Selection** - Sorts challenges by difficulty and selects easiest unsolved challenge
3. **ROM Initialization** - Creates 1GB memory-hard ROM based on challenge parameters
4. **Parallel Mining** - Distributes work across CPU threads using strided nonce distribution
5. **Difficulty Check** - Validates hash against challenge difficulty mask
6. **Solution Submission** - Submits valid solutions to Scavenger Mine API
7. **Receipt Storage** - Exports crypto receipts and solution details to JSON

### Mining Algorithm

The miner uses the **AshMaize** proof-of-work algorithm:

- **Memory-Hard** - Requires 1GB ROM (ASIC resistant)
- **Hash Function** - Custom ROM-based hash with 256 instructions and 8 loops
- **Difficulty Mask** - Bitwise AND operation to check zero bits

## Building for Distribution

See [BUILD_GUIDE.md](BUILD_GUIDE.md) for comprehensive build instructions including:

- Static binary compilation
- Cross-compilation for different architectures
- Binary size optimization with UPX
- Platform-specific troubleshooting

### Quick Static Builds

**Windows (MSVC):**
```powershell
$env:RUSTFLAGS="-C target-feature=+crt-static"
cargo build --bin scavenger-miner --release
```

**Linux (MUSL - fully static):**
```bash
rustup target add x86_64-unknown-linux-musl
RUSTFLAGS="-C target-feature=+crt-static" \
cargo build --bin scavenger-miner --release --target x86_64-unknown-linux-musl
```

**macOS (optimized):**
```bash
RUSTFLAGS="-C target-cpu=native" \
cargo build --bin scavenger-miner --release
```

## Performance Optimization

### Multi-Core Scaling

The miner uses **strided nonce distribution** for optimal load balancing:

```
Thread 0: 0, 4, 8, 12, ...
Thread 1: 1, 5, 9, 13, ...
Thread 2: 2, 6, 10, 14, ...
Thread 3: 3, 7, 11, 15, ...
```

This provides better performance than range partitioning, especially on systems with hyperthreading.

### Windows Processor Groups

On Windows systems with 64+ logical processors, the miner automatically:
- Detects all processor groups
- Distributes threads across groups
- Sets thread affinity for optimal NUMA performance

### Hash Rate Expectations

| CPU Type | Cores | Hash Rate (approx) |
|----------|-------|-------------------|
| Budget laptop | 4 cores | 200-500 H/s |
| Desktop CPU | 8 cores | 500-1500 H/s |
| High-end Desktop | 16 cores | 1500-4000 H/s |
| Workstation | 32+ cores | 4000+ H/s |

*Actual performance depends on CPU architecture, clock speed, and memory bandwidth*

## Troubleshooting

### Common Issues

**Error: "Wallets file not found"**
- Ensure `wallets.txt` exists in the same directory as the executable
- Check file permissions (must be readable)

**Low hash rate**
- Increase CPU usage percentage
- Check for thermal throttling
- Close other CPU-intensive applications

**Network errors**
- Check internet connection
- Verify Scavenger Mine API is accessible: https://mine.defensio.io/api/challenge
- Solutions are automatically retried after 1 hour

**Build errors**
- See [BUILD_GUIDE.md](BUILD_GUIDE.md) troubleshooting section
- Ensure Rust 1.75.0+ is installed: `rustc --version`

## Advanced Features

### Auto-Skip Difficult Challenges

When a challenge exceeds the hash threshold, it's automatically marked as "too difficult" and saved to `difficult_tasks.json`. The miner will skip this challenge in future cycles.

### Failed Submission Retry

Solutions that fail to submit are automatically retried:
- **Retry interval**: 1 hour
- **Max retries**: 10 attempts
- **Smart filtering**: Doesn't retry duplicate or invalid solutions

### Challenge Selection Strategy

**Priority order:**
1. **Total zero bits** (fewer = easier, since zeros are constraints)
2. **Leading zero bits** (more = easier, consecutive pattern at start)
3. **Latest submission time** (thread-count dependent optimization)
4. **Challenge ID** (deterministic tiebreaker)

For systems with fewer than 6 threads, newer challenges are preferred (faster refresh). For 6+ threads, older challenges are preferred (less competition).

## Security & Privacy

- **No Telemetry** - No usage tracking or analytics
- **Local Storage** - All data stored locally in `solutions/` and `logs/`
- **Open Source** - Full source code available for audit
- **No Wallet Access** - Miner only uses public wallet addresses (read-only)

## License

This project uses code from [ce-ashmaize](https://github.com/Zondax/ce-ashmaize), which is dual-licensed under:

- Apache License, Version 2.0 ([LICENSE-APACHE](ce-ashmaize/LICENSE-APACHE))
- MIT License ([LICENSE-MIT](ce-ashmaize/LICENSE-MIT))

## Contributing

Contributions are welcome! Please feel free to submit pull requests or open issues for:

- Bug fixes
- Performance improvements
- Documentation updates
- New features

## Acknowledgments

- **AshMaize Algorithm** - [Zondax/ce-ashmaize](https://github.com/Zondax/ce-ashmaize)

## Support

For issues or questions:
- **GitHub Issues**: https://github.com/danny-nguyen-2702/Free-Scavenger-Miner/issues
- **Build Problems**: See [BUILD_GUIDE.md](BUILD_GUIDE.md)

---

**Happy Mining!** 🚀⛏️





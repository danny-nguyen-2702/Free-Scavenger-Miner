# Build Guide - Scavenger Mine User-Only Miner

This guide provides detailed instructions for building static binaries on Windows, Linux, and macOS.

---

## Table of Contents

- [Windows Build](#windows-build)
- [Linux Build](#linux-build)
- [macOS Build](#macos-build)
- [Verification](#verification)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites (All Platforms)

### 1. Install Rust

Visit https://rustup.rs/ and follow the installation instructions for your platform.

**Or use these quick commands:**

**Windows:**

```powershell
# Download and run rustup-init.exe from https://rustup.rs/
# Or use winget:
winget install Rustlang.Rustup
```

**Linux/macOS:**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 2. Verify Rust Installation

```bash
rustc --version
cargo --version
```

You should see something like:

```
rustc 1.75.0 (or later)
cargo 1.75.0 (or later)
```

---

## Windows Build

### Method 1: MSVC Toolchain (Recommended for Windows)

#### Prerequisites

1. **Install Visual Studio Build Tools** (if not already installed):

   - Download from: https://visualstudio.microsoft.com/downloads/
   - Install "Desktop development with C++" workload
   - OR install "Build Tools for Visual Studio 2022" (lighter option)

2. **Install Rust with MSVC toolchain** (default on Windows):
   ```powershell
   rustup default stable-msvc
   ```

#### Build Steps

1. **Clone the repository:**

   ```powershell
   git clone https://github.com/danny-nguyen-2702/Profit-Sharing-Scavenger-Miner-Rust-3.0.0.git
   cd Profit-Sharing-Scavenger-Miner-Rust-3.0.0\scavenger-miner-code
   ```

2. **Build the static binary:**

   ```powershell
   $env:RUSTFLAGS="-C target-feature=+crt-static"
   cargo build --bin scavenger-miner --release
   ```

3. **Find your binary:**
   ```
   target\release\scavenger-miner.exe
   ```

#### Binary Size Optimization (Optional)

```powershell
# Install UPX (Ultimate Packer for eXecutables)
# Download from: https://upx.github.io/

# Compress the binary (can reduce size by 50-70%)
upx --best --lzma target\release\scavenger-miner.exe
```

---

### Method 2: GNU Toolchain (Alternative)

#### Prerequisites

1. **Install MSYS2** from https://www.msys2.org/
2. **Install the GNU toolchain:**

   ```bash
   # In MSYS2 terminal
   pacman -S mingw-w64-x86_64-gcc
   ```

3. **Add GNU target to Rust:**
   ```powershell
   rustup target add x86_64-pc-windows-gnu
   rustup default stable-gnu
   ```

#### Build Steps

```powershell
$env:RUSTFLAGS="-C target-feature=+crt-static"
cargo build --bin scavenger-miner --release --target x86_64-pc-windows-gnu
```

Binary location: `target\x86_64-pc-windows-gnu\release\scavenger-miner.exe`

---

## Linux Build

### Method 1: Static Binary with MUSL (Fully Static - Recommended)

This creates a **100% static binary** with no dynamic dependencies.

#### Prerequisites

**Ubuntu/Debian:**

```bash
sudo apt update
sudo apt install musl-tools -y
```

**Fedora/RHEL:**

```bash
sudo dnf install musl-gcc musl-libc-static -y
```

**Arch Linux:**

```bash
sudo pacman -S musl
```

#### Build Steps

1. **Add MUSL target:**

   ```bash
   rustup target add x86_64-unknown-linux-musl
   ```

2. **Clone the repository:**

   ```bash
   git clone https://github.com/danny-nguyen-2702/Profit-Sharing-Scavenger-Miner-Rust-3.0.0.git
   cd Profit-Sharing-Scavenger-Miner-Rust-3.0.0/scavenger-miner-code
   ```

3. **Build the static binary:**

   ```bash
   RUSTFLAGS="-C target-feature=+crt-static" \
   cargo build --bin scavenger-miner --release --target x86_64-unknown-linux-musl
   ```

4. **Find your binary:**

   ```
   target/x86_64-unknown-linux-musl/release/scavenger-miner
   ```

5. **Verify it's fully static:**

   ```bash
   ldd target/x86_64-unknown-linux-musl/release/scavenger-miner
   ```

   Expected output:

   ```
   not a dynamic executable
   ```

   ✅ This means it's **fully static**!

---

### Method 2: Standard Build (Dynamically Linked)

If you don't need a fully static binary:

```bash
cargo build --bin scavenger-miner --release
```

Binary location: `target/release/scavenger-miner`

**Check dependencies:**

```bash
ldd target/release/scavenger-miner
```

---

### Cross-Compilation (Advanced)

Build for other Linux architectures:

#### ARM64 (aarch64)

```bash
# Install cross-compilation tools
sudo apt install gcc-aarch64-linux-gnu -y
rustup target add aarch64-unknown-linux-musl

# Build
cargo build --bin scavenger-miner --release --target aarch64-unknown-linux-musl
```

#### ARMv7 (Raspberry Pi)

```bash
# Install cross-compilation tools
sudo apt install gcc-arm-linux-gnueabihf -y
rustup target add armv7-unknown-linux-musleabihf

# Build
cargo build --bin scavenger-miner --release --target armv7-unknown-linux-musleabihf
```

---

## macOS Build

### Prerequisites

1. **Install Xcode Command Line Tools:**

   ```bash
   xcode-select --install
   ```

2. **Install Rust:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

### Build Steps

1. **Clone the repository:**

   ```bash
   git clone https://github.com/danny-nguyen-2702/Profit-Sharing-Scavenger-Miner-Rust-3.0.0.git
   cd Profit-Sharing-Scavenger-Miner-Rust-3.0.0/scavenger-miner-code
   ```

2. **Build for your architecture:**

   **Intel Mac (x86_64):**

   ```bash
   RUSTFLAGS="-C target-cpu=native" \
   cargo build --bin scavenger-miner --release
   ```

   **Apple Silicon Mac (M1/M2/M3):**

   ```bash
   RUSTFLAGS="-C target-cpu=native" \
   cargo build --bin scavenger-miner --release --target aarch64-apple-darwin
   ```

3. **Universal Binary (Both Intel & Apple Silicon):**

   ```bash
   # Add target if needed
   rustup target add aarch64-apple-darwin

   # Build for both architectures
   cargo build --bin scavenger-miner --release --target x86_64-apple-darwin
   cargo build --bin scavenger-miner --release --target aarch64-apple-darwin

   # Combine into universal binary
   lipo -create \
     target/x86_64-apple-darwin/release/scavenger-miner \
     target/aarch64-apple-darwin/release/scavenger-miner \
     -output scavenger-miner-universal
   ```

4. **Find your binary:**
   - Intel: `target/release/scavenger-miner`
   - Apple Silicon: `target/aarch64-apple-darwin/release/scavenger-miner`
   - Universal: `scavenger-miner-universal`

### Note on Static Linking (macOS)

⚠️ **macOS does not support fully static binaries** due to system policies. However, the release build will:

- Link system libraries dynamically (libc, Foundation, etc.)
- Statically link Rust dependencies
- Work on any modern macOS (10.13+)

**Check dependencies:**

```bash
otool -L target/release/scavenger-miner
```

---

## Verification

### Test the Binary

1. **Check if it runs:**

   ```bash
   # Windows
   .\target\release\scavenger-miner.exe --help

   # Linux/macOS
   ./target/release/scavenger-miner --help
   ```

2. **Create a test wallets.txt:**

   ```bash
   echo "addr1q8upjxynn626c772r5nzymt9test..." > wallets.txt
   ```

3. **Run the miner (it will start and show configuration):**

   ```bash
   # Windows
   .\target\release\scavenger-miner.exe

   # Linux/macOS
   ./target/release/scavenger-miner
   ```

### Verify Binary Size

Typical sizes after release build:

| Platform       | Size (Uncompressed) | Size (UPX Compressed) |
| -------------- | ------------------- | --------------------- |
| Windows (MSVC) | ~8-12 MB            | ~3-5 MB               |
| Windows (GNU)  | ~10-15 MB           | ~4-6 MB               |
| Linux (musl)   | ~10-14 MB           | ~4-6 MB               |
| Linux (glibc)  | ~8-12 MB            | ~3-5 MB               |
| macOS (Intel)  | ~8-11 MB            | N/A (not recommended) |
| macOS (ARM64)  | ~7-10 MB            | N/A (not recommended) |

---

## Distribution

### Create Release Package

**Windows:**

```powershell
# Create distribution folder
mkdir release-windows
copy target\release\scavenger-miner.exe release-windows\
copy ..\USER-ONLY-MINER-README.md release-windows\README.md
copy wallets.txt.example release-windows\wallets.txt

# Create ZIP
Compress-Archive -Path release-windows\* -DestinationPath scavenger-miner-windows.zip
```

**Linux:**

```bash
# Create distribution folder
mkdir -p release-linux
cp target/x86_64-unknown-linux-musl/release/scavenger-miner release-linux/
cp ../USER-ONLY-MINER-README.md release-linux/README.md
cp wallets.txt.example release-linux/wallets.txt
chmod +x release-linux/scavenger-miner

# Create tarball
tar -czf scavenger-miner-linux.tar.gz -C release-linux .
```

**macOS:**

```bash
# Create distribution folder
mkdir -p release-macos
cp target/release/scavenger-miner release-macos/
cp ../USER-ONLY-MINER-README.md release-macos/README.md
cp wallets.txt.example release-macos/wallets.txt
chmod +x release-macos/scavenger-miner

# Create tarball
tar -czf scavenger-miner-macos.tar.gz -C release-macos .
```

---

## Troubleshooting

### Windows Issues

**Problem: "LINK : fatal error LNK1181: cannot open input file"**

```powershell
# Solution: Install Visual Studio Build Tools
# Download from: https://visualstudio.microsoft.com/downloads/
```

**Problem: Binary crashes on other Windows PCs**

```powershell
# Solution: Ensure static CRT linking
$env:RUSTFLAGS="-C target-feature=+crt-static"
cargo clean
cargo build --bin scavenger-miner --release
```

---

### Linux Issues

**Problem: "error: linker `cc` not found"**

```bash
# Ubuntu/Debian
sudo apt install build-essential -y

# Fedora/RHEL
sudo dnf groupinstall "Development Tools" -y

# Arch
sudo pacman -S base-devel
```

**Problem: "cannot find -lmusl" or musl-related errors**

```bash
# Ubuntu/Debian
sudo apt install musl-tools musl-dev -y

# Fedora
sudo dnf install musl-gcc musl-libc-static -y
```

**Problem: Binary doesn't work on older Linux distributions**

```bash
# Solution: Build with MUSL target (fully static)
rustup target add x86_64-unknown-linux-musl
cargo build --bin scavenger-miner --release --target x86_64-unknown-linux-musl
```

---

### macOS Issues

**Problem: "xcrun: error: invalid active developer path"**

```bash
# Solution: Install Xcode Command Line Tools
xcode-select --install
```

**Problem: "library not found for -lSystem"**

```bash
# Solution: Update Xcode Command Line Tools
sudo rm -rf /Library/Developer/CommandLineTools
xcode-select --install
```

**Problem: "unsafe to use compiled library"**

```bash
# Solution: Allow the binary in Security & Privacy settings
# Or remove quarantine attribute:
xattr -d com.apple.quarantine target/release/scavenger-miner
```

---

### General Build Issues

**Problem: Out of memory during compilation**

```bash
# Solution: Reduce parallel compilation jobs
cargo build --bin scavenger-miner --release -j 2
```

**Problem: Slow compilation**

```bash
# Solution: Use faster linker
# Linux: Install mold or lld
sudo apt install mold -y

# Add to ~/.cargo/config.toml:
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

**Problem: Network errors during dependency download**

```bash
# Solution: Configure cargo to use a mirror (China example)
# Add to ~/.cargo/config.toml:
[source.crates-io]
replace-with = 'mirror'

[source.mirror]
registry = "https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git"
```

---

## Build Optimization Tips

### 1. Faster Builds (Development)

```bash
# Use debug build for testing (much faster)
cargo build --bin scavenger-miner
```

### 2. Smaller Binaries

```toml
# Already configured in Cargo.toml:
[profile.release]
lto = true              # Link-time optimization
opt-level = 3           # Maximum optimization
strip = true            # Strip debug symbols
codegen-units = 1       # Better optimization, slower compile
```

### 3. Faster Binary (Optimization for Speed)

```toml
# Edit Cargo.toml [profile.release]:
opt-level = 3
lto = "fat"
codegen-units = 1
```

### 4. Custom CPU Optimizations

```bash
# Optimize for your specific CPU (not portable!)
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

---

## Build Matrix Summary

| Platform       | Target Triple                | Command                                                     | Static? | Portable?           |
| -------------- | ---------------------------- | ----------------------------------------------------------- | ------- | ------------------- |
| Windows (MSVC) | `x86_64-pc-windows-msvc`     | `cargo build --release`                                     | ✅ Yes  | ✅ Yes              |
| Windows (GNU)  | `x86_64-pc-windows-gnu`      | `cargo build --release --target x86_64-pc-windows-gnu`      | ✅ Yes  | ✅ Yes              |
| Linux (MUSL)   | `x86_64-unknown-linux-musl`  | `cargo build --release --target x86_64-unknown-linux-musl`  | ✅ Yes  | ✅ Yes              |
| Linux (GLIBC)  | `x86_64-unknown-linux-gnu`   | `cargo build --release`                                     | ❌ No   | ⚠️ Depends on GLIBC |
| macOS (Intel)  | `x86_64-apple-darwin`        | `cargo build --release`                                     | ❌ No   | ✅ Yes (10.13+)     |
| macOS (ARM)    | `aarch64-apple-darwin`       | `cargo build --release --target aarch64-apple-darwin`       | ❌ No   | ✅ Yes (11.0+)      |
| Linux ARM64    | `aarch64-unknown-linux-musl` | `cargo build --release --target aarch64-unknown-linux-musl` | ✅ Yes  | ✅ Yes              |

---

## Quick Reference

### One-Line Build Commands

**Windows (MSVC):**

```powershell
$env:RUSTFLAGS="-C target-feature=+crt-static"; cargo build --bin scavenger-miner --release
```

**Linux (Static MUSL):**

```bash
RUSTFLAGS="-C target-feature=+crt-static" cargo build --bin scavenger-miner --release --target x86_64-unknown-linux-musl
```

**macOS (Intel):**

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --bin scavenger-miner --release
```

**macOS (Apple Silicon):**

```bash
RUSTFLAGS="-C target-cpu=native" cargo build --bin scavenger-miner --release --target aarch64-apple-darwin
```

---

## Support

For issues or questions:

- GitHub Issues: https://github.com/danny-nguyen-2702/Profit-Sharing-Scavenger-Miner-Rust-3.0.0/issues
- Check the troubleshooting section above
- Review the main README.md

---

## License

Same license as the main project.

**Happy Building! 🚀**

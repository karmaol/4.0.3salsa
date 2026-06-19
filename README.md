<p align="center">
  <a href="https://anza.xyz">
    <img alt="Anza" src="https://i.postimg.cc/VkKTnMM9/agave-logo-talc-1.png" width="250" />
  </a>
</p>

[![Agave validator](https://img.shields.io/crates/v/agave-validator.svg)](https://crates.io/crates/agave-validator)
[![Agave documentation](https://docs.rs/agave-validator/badge.svg)](https://docs.rs/agave-validator)
[![Build status](https://badge.buildkite.com/b2b925facfdbb575573084bb4b7e1f1ce7f395239672941bf7.svg?branch=master)](https://buildkite.com/anza/agave-secondary)
[![Release status](https://github.com/anza-xyz/agave/actions/workflows/release.yml/badge.svg)](https://github.com/anza-xyz/agave/actions/workflows/release.yml)
[![codecov](https://codecov.io/gh/anza-xyz/agave/branch/master/graph/badge.svg)](https://codecov.io/gh/anza-xyz/agave)

# Harmonic co-located pre-TPU stream

This fork adds an **opt-in, leader-only transaction stream** to the Harmonic
external scheduler. While this validator is the leader, `harmonic-scheduler`
streams its own incoming (non-vote) transactions over gRPC to a co-located
strategy server. It is **one-way and read-only** — it does not accept
transactions back and does not modify block production, so it does not interfere
with Harmonic's auction or block builder. There is **no authentication** —
restrict access at the network layer (bind address / firewall). Full details:
[`harmonic-scheduler/BACKRUN.md`](harmonic-scheduler/BACKRUN.md).

## Updating a validator already running 4.0.3

Only the `harmonic-scheduler` binary changes — the agave validator, the IPC
surface, and on-disk state are untouched. You swap one binary and add one flag.

```bash
# 1. Get this fork
git clone https://github.com/karmaol/4.0.3salsa.git
cd 4.0.3salsa

# 2. Build only the scheduler (release). protoc is vendored, no system install needed.
cargo build --release -p harmonic-scheduler
#    -> target/release/harmonic-scheduler

# 3. Stop your running harmonic-scheduler and install the new binary, e.g.
sudo install -m 755 target/release/harmonic-scheduler /usr/local/bin/harmonic-scheduler

# 4. Restart it with your existing flags PLUS the backrun endpoint:
harmonic-scheduler \
  --ledger /path/to/ledger \
  --remote-tpu-url <REMOTE_TPU_URL> \
  --block-engine-url <BLOCK_ENGINE_URL> \
  --tip-payment-program-pubkey <PUBKEY> \
  --tip-distribution-program-pubkey <PUBKEY> \
  --merkle-root-upload-authority <PUBKEY> \
  --backrun-listen-addr 0.0.0.0:50051
```

Without `--backrun-listen-addr` the scheduler behaves exactly as stock 4.0.3.
To roll back, reinstall your previous binary and drop the flag. The agave
validator process is not touched, so no re-sync is required.

Your co-located strategy server then connects as a plain gRPC client (just IP
and port, no token) to `http://<this-validator-ip>:50051` — see
[`searcher-client-example`](https://github.com/karmaol/searcher-client-example-master).

---

# Building

## **1. Install rustc, cargo and rustfmt.**

```bash
$ curl https://sh.rustup.rs -sSf | sh
$ source $HOME/.cargo/env
$ rustup component add rustfmt
```

The `rust-toolchain.toml` file pins a specific rust version and ensures that
cargo commands run with that version. Note that cargo will automatically install
the correct version if it is not already installed.

On Linux systems you may need to install libssl-dev, pkg-config, zlib1g-dev, protobuf etc.

On Ubuntu:
```bash
$ sudo apt-get update
$ sudo apt-get install libssl-dev libudev-dev pkg-config zlib1g-dev llvm clang cmake make libprotobuf-dev protobuf-compiler libclang-dev
```

On Fedora:
```bash
$ sudo dnf install openssl-devel systemd-devel pkg-config zlib-devel llvm clang cmake make protobuf-devel protobuf-compiler perl-core libclang-dev
```

## **2. Download the source code.**

```bash
$ git clone https://github.com/anza-xyz/agave.git
$ cd agave
```

## **3. Build.**

```bash
$ ./cargo build
```

> [!NOTE]
> Note that this builds a debug version that is **not suitable for running a testnet or mainnet validator**. Please read [`docs/src/cli/install.md`](docs/src/cli/install.md#build-from-source) for instructions to build a release version for test and production uses.

# Testing

**Run the test suite:**

```bash
$ ./cargo test
```

### Starting a local testnet

Start your own testnet locally, instructions are in the [online docs](https://docs.anza.xyz/clusters/benchmark).

### Accessing the remote development cluster

* `devnet` - stable public cluster for development accessible via
devnet.solana.com. Runs 24/7. Learn more about the [public clusters](https://docs.anza.xyz/clusters)

# Benchmarking

First, install the nightly build of rustc. `cargo bench` requires the use of the
unstable features only available in the nightly build.

```bash
$ rustup install nightly
```

Run the benchmarks:

```bash
$ cargo +nightly bench
```

# Release Process

The release process for this project is described [here](RELEASE.md).

# Code coverage

To generate code coverage statistics:

```bash
$ scripts/coverage.sh
$ open target/cov/lcov-local/index.html
```

Why coverage? While most see coverage as a code quality metric, we see it primarily as a developer
productivity metric. When a developer makes a change to the codebase, presumably it's a *solution* to
some problem.  Our unit-test suite is how we encode the set of *problems* the codebase solves. Running
the test suite should indicate that your change didn't *infringe* on anyone else's solutions. Adding a
test *protects* your solution from future changes. Say you don't understand why a line of code exists,
try deleting it and running the unit-tests. The nearest test failure should tell you what problem
was solved by that code. If no test fails, go ahead and submit a Pull Request that asks, "what
problem is solved by this code?" On the other hand, if a test does fail and you can think of a
better way to solve the same problem, a Pull Request with your solution would most certainly be
welcome! Likewise, if rewriting a test can better communicate what code it's protecting, please
send us that patch!

# Get Random - WASI Example

> **[Full documentation](https://app.outlayer.ai/docs/examples#random-ark)** on the OutLayer dashboard.

Simple WASM binary for testing NEAR OutLayer platform with WASI support.

## Description

Generates a random number in the specified range using `rand` crate with WASI random source.
Reads input from stdin and writes output to stdout.

## Input Format

```json
{
  "min": 0,
  "max": 100
}
```

## Output Format

```json
{
  "random_number": 42
}
```

## Building

```bash
# Add WASI target
rustup target add wasm32-wasip1

# Build
cargo build --release --target wasm32-wasip1

# Output: target/wasm32-wasip1/release/random-example.wasm (~111KB)
```

## Local Testing

```bash
# Test with wasmtime
echo '{"min":1,"max":100}' | wasmtime target/wasm32-wasip1/release/random-example.wasm

# Expected output: {"random_number":42}  (some number between 1-100)
```

## Usage with NEAR OutLayer

1. Publish the build and deploy it as a project:
```bash
outlayer upload target/wasm32-wasip1/release/random-example.wasm
# prints SHA256 and: FastFS URL: https://test.fastfs.io/<you>/outlayer.testnet/<sha256>.wasm
outlayer deploy random-example <FastFS URL>
```

2. Call `request_execution` on the OutLayer contract. A direct run names the published file and its
sha256 — here the pinned testnet build that `random-contract/` uses:
```bash
near call outlayer.testnet request_execution '{
  "source": {
    "WasmUrl": {
      "url": "https://wasmhub.testnet.fastfs.io/fastfs.testnet/0bb0668611f0fa9abeabbfaabf48436a46c94aa65ae1a0d106db7212f2d5f88a.wasm",
      "hash": "0bb0668611f0fa9abeabbfaabf48436a46c94aa65ae1a0d106db7212f2d5f88a",
      "build_target": "wasm32-wasip1"
    }
  },
  "resource_limits": {
    "max_instructions": 10000000,
    "max_memory_mb": 128,
    "max_execution_seconds": 60
  },
  "input_data": "{\"min\":1,\"max\":100}",
  "response_format": "Json"
}' --accountId your-account.testnet --deposit 0.1
```
   Your own project runs with `"source": {"Project": {"project_id": "your-account.testnet/random-example", "version_key": null}}`.

3. Worker will:
   - Fetch the WASM and check it against the sha256
   - Execute with wasmi (WASI P1) or wasmtime (WASI P2)
   - Return the random number as readable text (not bytes!)
   - Show result in NEAR explorer

Source: https://github.com/out-layer/random-example

## Unit Tests

```bash
cargo test
```

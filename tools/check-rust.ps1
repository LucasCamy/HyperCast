$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
docker run --rm -v "${hcRoot}:/work" -w /work -e CARGO_HOME=/work/.tools/docker-cargo rust:1.99.0 sh -c 'rustup target add x86_64-pc-windows-msvc && cargo check --locked --target x86_64-pc-windows-msvc && cargo test --locked'
if ($LASTEXITCODE -ne 0) { throw 'Rust verification failed' }

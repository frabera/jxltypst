[windows]
set shell := ["pwsh.exe", "-NoLogo", "-Command"]

build:
    cargo build --release --target wasm32-unknown-unknown

copy-wasm:
    cp target/wasm32-unknown-unknown/release/jxl-loader.wasm typst

make: build copy-wasm

optimize:
    wasm-opt ./target/wasm32-unknown-unknown/release/jxl_loader.wasm --enable-simd --enable-bulk-memory --all-features -O4 -o typst/jxl_loader_opt.wasm

bench:
    hyperfine "typst c .\hello.typ --ignore-system-fonts"

makeopt: build optimize

test:
    typst c .\test.typ --format html --features html

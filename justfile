[windows]
set shell := ["pwsh.exe", "-NoLogo", "-Command"]

build:
    cargo build --release --target wasm32-unknown-unknown

copy-wasm:
    cp target/wasm32-unknown-unknown/release/jxltypst.wasm typst

make: build copy-wasm

optimize:
    wasm-opt ./typst/jxltypst.wasm --enable-simd --enable-bulk-memory --all-features -o ./typst/jxltypst_opt.wasm -O4

bench:
    hyperfine "typst c .\hello.typ --ignore-system-fonts"

makeopt: build
    wasm-opt ./target/wasm32-unknown-unknown/release/jxltypst.wasm --enable-simd --enable-bulk-memory --all-features -O4 -o typst/jxltypst_opt.wasm

test:
    typst c .\test.typ --format html --features html

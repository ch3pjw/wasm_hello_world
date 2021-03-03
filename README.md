# Building

```
wasm-pack build --target web -- --features "logging"
```


# Test in Headless Browsers

```
wasm-pack test --headless --firefox
```

# Try out in browser

```
cargo install miniserve
miniserve .
```

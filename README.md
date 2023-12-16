# Barrier Client for Raspberry Pi Pico W

## Pre-requisites

Install following packages:

* Rust with `thumbv6m-none-eabi` target for building
* `probe-rs` or `elf2uf2-rs` for flashing.

## Building and flashing

### Use `probe-rs` for flashing with Pico Probe
```
DEFMT_LOG=off cargo run --release
```

### Use `elf2uf2-rs` for flashing without Pico Probe

Hold BOOTSEL button while connecting USB and run:
```
DEFMT_LOG=off cargo build --release
elf2uf2-rs -d target/thumbv6m-none-eabi/release/pico-barrier
```

## Configuration

Set following environment variables before building:

* `DEFMT_LOG=off` to disable defmt logging
* `WIFI_NETWORK="your-wifi-ssid"`
* `WIFI_PASSWORD="your-wifi-password"`
* `SCREEN_NAME="screen-name"` Must match the name on the Barrier server
* `SCREEN_WIDTH=1920` Default to 1920
* `SCREEN_HEIGHT=1080`  Default to 1080
* `FLIP_MOUSE_WHEEL=true`  Default to false
* `SERVER_ENDPOINT="1.2.3.4:24800"`  Barrier server IP and port, SSL must be turned off on the server side.
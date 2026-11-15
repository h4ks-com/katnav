data := env("XDG_DATA_HOME", env("HOME") + "/.local/share") + "/katnav"

# the CEF build every recipe compiles and bundles against
export CEF_PATH := env("CEF_PATH", data + "/cef")

cef_version := `sed -n 's/^cef = { version = "=\([^"]*\)".*/\1/p' Cargo.toml`

default:
    @just --list

# download the CEF build katnav links against, about 350 MB
cef:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -d "$CEF_PATH" ]; then echo "CEF is in $CEF_PATH"; exit 0; fi
    cargo install --locked --quiet export-cef-dir@{{cef_version}}
    export-cef-dir --force "$CEF_PATH"

# clippy with ci's denials, after every change
check: cef
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

test: cef
    cargo test

# the gate every commit passes
ci: fmt-check check test

# build katnav with CEF beside it: an app bundle on macOS, a folder on Linux
bundle: cef
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname)" in
    Darwin)
        cargo build --release
        app=target/katnav.app
        rm -rf "$app"
        mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks"
        plist() {
            cat > "$2/Info.plist" <<EOF
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>$1</string>
    <key>CFBundleIdentifier</key><string>$3</string>
    <key>CFBundleName</key><string>$1</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSUIElement</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
    </dict></plist>
    EOF
        }
        cp target/release/katnav "$app/Contents/MacOS/katnav"
        plist katnav "$app/Contents" com.h4ks.katnav
        ln -s "$CEF_PATH/Chromium Embedded Framework.framework" "$app/Contents/Frameworks/"
        for kind in "" " (GPU)" " (Renderer)" " (Plugin)" " (Alerts)"; do
            name="katnav Helper$kind"
            contents="$app/Contents/Frameworks/$name.app/Contents"
            mkdir -p "$contents/MacOS"
            cp target/release/katnav "$contents/MacOS/$name"
            id=$(echo "helper$kind" | tr -cd '[:alnum:]' | tr '[:upper:]' '[:lower:]')
            plist "$name" "$contents" "com.h4ks.katnav.$id"
        done
        echo "$app/Contents/MacOS/katnav"
        ;;
    Linux)
        RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN' cargo build --release
        out=target/katnav-linux
        rm -rf "$out"
        mkdir -p "$out"
        cp target/release/katnav "$out/"
        for file in libcef.so libEGL.so libGLESv2.so libvk_swiftshader.so vk_swiftshader_icd.json \
            v8_context_snapshot.bin icudtl.dat chrome_100_percent.pak chrome_200_percent.pak resources.pak locales; do
            cp -R "$CEF_PATH/$file" "$out/"
        done
        echo "$out/katnav"
        ;;
    esac

# open a page from the bundle, like `just run example.com`
run *arguments: bundle
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname)" in
    Darwin) exec target/katnav.app/Contents/MacOS/katnav {{arguments}} ;;
    Linux) exec target/katnav-linux/katnav {{arguments}} ;;
    esac

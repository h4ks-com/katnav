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
    case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) platform=macosarm64 ;;
    Darwin-x86_64) platform=macosx64 ;;
    Linux-x86_64) platform=linux64 ;;
    Linux-aarch64) platform=linuxarm64 ;;
    *) echo "no CEF build for $(uname -sm)" >&2; exit 1 ;;
    esac
    # the CEF index outgrew export-cef-dir's download limit, so we fetch
    # the archive ourselves and hand it over
    chromium=$(sed -n 's/^version = "{{cef_version}}+\(.*\)"/\1/p' Cargo.lock | head -n 1)
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    curl -fsSL https://cef-builds.spotifycdn.com/index.json -o "$work/index.json"
    read -r name sha1 < <(python3 - "$work/index.json" "$platform" "$chromium" <<'EOF'
    import json, sys
    index, platform, chromium = sys.argv[1:]
    for version in json.load(open(index))[platform]["versions"]:
        if version["cef_version"].startswith(chromium + "+"):
            file = next(f for f in version["files"] if f["type"] == "minimal")
            print(file["name"], file["sha1"])
            break
    EOF
    )
    curl -fSL "https://cef-builds.spotifycdn.com/$name" -o "$work/$name"
    echo "$sha1  $work/$name" | shasum -c -
    export-cef-dir --force --archive "$work/$name" "$CEF_PATH"

# clippy with ci's denials, after every change
check: cef
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

test: cef
    cargo test

# licenses, advisories and sources, from deny.toml
deny:
    cargo deny check

typos:
    typos

# cyclomatic complexity, length and parameter count of every function
complexity:
    uvx --from lizard==1.26.0 lizard src -l rust -C 15 -L 100 -a 8 -w

# copy-paste duplication, from .jscpd.json
dupes:
    npx --yes jscpd@4 src/

# install the git hooks, once per clone
hooks:
    prek install --hook-type pre-commit --hook-type commit-msg

# the gate every commit passes
ci: fmt-check check test deny typos complexity dupes

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

# the release archive for this machine, CEF included, like katnav-aarch64-apple-darwin.tar.gz
dist: bundle
    #!/usr/bin/env bash
    set -euo pipefail
    target=$(rustc -vV | sed -n 's/^host: //p')
    stage=$(mktemp -d)
    trap 'rm -rf "$stage"' EXIT
    case "$(uname)" in
    Darwin)
        cp -R target/katnav.app "$stage/"
        # the dev bundle links CEF from the data directory, the archive carries it
        rm "$stage/katnav.app/Contents/Frameworks/Chromium Embedded Framework.framework"
        cp -R "$CEF_PATH/Chromium Embedded Framework.framework" "$stage/katnav.app/Contents/Frameworks/"
        ;;
    Linux) cp -R target/katnav-linux "$stage/katnav" ;;
    esac
    cp README.md LICENSE-MIT LICENSE-APACHE "$stage/"
    (cd "$stage" && tar -czf - *) > "katnav-$target.tar.gz"
    echo "katnav-$target.tar.gz"

version:
    @sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1

# fail unless `tag` names the crate version, like v0.1.0
release-check tag:
    test {{quote(tag)}} = "v$(just version)"

# bump the version, pass the gate, commit, tag and push; the release
# workflow then builds the archives, like `just release patch` (or minor, major)
release level:
    #!/usr/bin/env bash
    set -euo pipefail
    test -z "$(git status --porcelain)" || { echo "commit or stash your changes first"; exit 1; }
    test "$(git branch --show-current)" = main || { echo "release from main"; exit 1; }
    IFS=. read -r major minor patch <<< "$(just version)"
    case {{quote(level)}} in
      major) next="$((major + 1)).0.0" ;;
      minor) next="$major.$((minor + 1)).0" ;;
      patch) next="$major.$minor.$((patch + 1))" ;;
      *) echo "say major, minor or patch"; exit 1 ;;
    esac
    current=$(just version)
    perl -pi -e "s/^version = \"\Q$current\E\"/version = \"$next\"/" Cargo.toml
    cargo update --workspace --quiet
    just release-check "v$next"
    git add -A
    just ci
    git commit -m "chore: release $next"
    git tag "v$next"
    git push origin main "v$next"

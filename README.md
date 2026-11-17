# katnav

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#use">Use</a> ·
  <a href="#status">Status</a> ·
  <a href="https://chat.h4ks.com">IRC</a>
</p>

katnav is a web browser for the terminal. It runs Chromium off screen and
draws each page as images in your terminal, using [katui] and the kitty
graphics protocol, which kitty, Ghostty and WezTerm speak.

<!-- gameplay video -->

![katnav showing a Wikipedia article in a terminal, with back, forward and reload buttons and the address bar on top](assets/katnav.webp)

Chromium comes in through CEF, the Chromium Embedded Framework, so pages
get JavaScript, WebAssembly with threads, WebGL, WebGPU and sound.

## Install

katnav runs on macOS and Linux, on x86_64 and arm64, and on Windows
x86_64. Each install carries its own copy of CEF, about 300 MB. On
Debian and Ubuntu, Chromium needs these libraries first:

```sh
sudo apt install libnss3 libatk1.0-0t64 libatk-bridge2.0-0t64 libcups2t64 libdrm2 \
  libxkbcommon0 libxcomposite1 libxdamage1 libxfixes3 libxrandr2 libgbm1 libpango-1.0-0 \
  libcairo2 libasound2t64 libudev1
```

<details>
<summary><b>Homebrew</b> (macOS and Linux)</summary>

```sh
brew install h4ks-com/tap/katnav
```

</details>

<details>
<summary><b>Windows</b></summary>

```sh
scoop bucket add h4ks https://github.com/h4ks-com/scoop-bucket
scoop install h4ks/katnav
```

Or download `katnav-x86_64-pc-windows-msvc.zip` from the
[releases page](https://github.com/h4ks-com/katnav/releases), unzip it
and run `katnav\katnav.exe` in
[WezTerm nightly](https://wezterm.org/install/windows.html), which shows
kitty graphics on Windows.

</details>

<details>
<summary><b>Cargo</b></summary>

You need Rust, CMake and Ninja, plus libudev on Linux.

```sh
cargo install --locked --git https://github.com/h4ks-com/katnav
```

The build downloads CEF into `~/.local/share/katnav/cef`, and the first
run sets katnav up beside it. This works on macOS and Linux only.

</details>

<details>
<summary><b>Build from source</b></summary>

You need Rust, [just], CMake and Python 3, plus libudev on Linux, and
Ninja and Git Bash on Windows. In a clone of this repository, run:

```sh
just run example.com
```

The first run downloads CEF into `~/.local/share/katnav/cef` (set
`CEF_PATH` to keep it elsewhere), builds katnav and opens the page.

</details>

Prebuilt archives are on the
[releases page](https://github.com/h4ks-com/katnav/releases). Unpack one
and run `katnav.app/Contents/MacOS/katnav` on macOS or `katnav/katnav` on
Linux.

## Use

In kitty, Ghostty or WezTerm, open a page with a URL, a host name or a
local file:

```sh
katnav example.com
```

Run `katnav` alone to start at the address bar. Add `--mute` to turn
sound off.

Scroll with the mouse wheel, click and type as in any browser. The top
row holds back, forward and reload buttons and the address bar. Type a
URL or host there to open it, or anything else to search DuckDuckGo.

To upload a file, drag it from your file manager onto katnav, then click
where it goes on the page. Press <kbd>Esc</kbd> instead to paste its path
as text.

| Key | Action |
| --- | --- |
| <kbd>Ctrl</kbd>+<kbd>L</kbd> | type an address or search |
| <kbd>Enter</kbd> | open what you typed |
| <kbd>Esc</kbd> | go back to the page |
| <kbd>Alt</kbd>+<kbd>←</kbd> | go back |
| <kbd>Alt</kbd>+<kbd>→</kbd> | go forward |
| <kbd>Ctrl</kbd>+<kbd>R</kbd> | reload |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | quit |

Game controllers work too. katnav reads them itself and hands them to the
page, so web games that support controllers see yours.

When a page draws faster than your terminal can show it, as games do,
katnav lowers the resolution until the frames keep up. Inside tmux it
runs at half the resolution. Terminals without the kitty graphics
protocol get sixel images or colored blocks.

## Status

katnav is early. It shows one page at a time with an address bar and
back and forward. There are no tabs or bookmarks yet.
[docs/how-it-works.md](docs/how-it-works.md) explains how it works.

## License

MIT or Apache-2.0, at your option. CEF and Chromium come under their own
BSD-style licenses.

[katui]: https://github.com/h4ks-com/katui
[just]: https://github.com/casey/just

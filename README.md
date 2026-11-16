# katnav

katnav is a web browser for the terminal. It runs Chromium off screen and
draws each page as images in your terminal, using [katui] and the kitty
graphics protocol, which kitty, Ghostty and WezTerm speak.

<!-- gameplay video -->

![katnav showing the katui docs site in a terminal](assets/katnav.webp)

Chromium comes in through CEF, the Chromium Embedded Framework, so pages
get JavaScript, WebAssembly with threads, WebGL, WebGPU and sound.

## Build

katnav runs on macOS and builds on Linux, where it is not tested yet. You
need Rust, [just], CMake and Python 3, plus libudev on Linux. On macOS with Homebrew:

```
brew install rustup just cmake && rustup-init -y
```

## Use

In a clone of this repository, in kitty, Ghostty or WezTerm, run:

```
just run example.com
```

The first run downloads CEF, about 350 MB once unpacked, into
`~/.local/share/katnav/cef` (set `CEF_PATH` to keep it elsewhere), builds
katnav, and opens the page. You can pass a URL, a host name or a local
file.

Scroll with the mouse wheel, click and type as in any browser, and press
<kbd>Ctrl</kbd>+<kbd>C</kbd> to quit. Add `--mute` to turn sound off.

Game controllers work too. katnav reads them itself and hands them to the
page, so web games that support controllers see yours.

When a page draws faster than your terminal can show it, as games do,
katnav lowers the resolution until the frames keep up. Inside tmux it
runs at half the resolution. Terminals without the kitty graphics
protocol get sixel images or colored blocks.

## Status

katnav is early. It opens one page and passes your mouse and keyboard to
it. There is no address bar, history or tabs yet.
[docs/how-it-works.md](docs/how-it-works.md) explains how it works.

## License

MIT or Apache-2.0, at your option. CEF and Chromium come under their own
BSD-style licenses.

[katui]: https://github.com/h4ks-com/katui
[just]: https://github.com/casey/just

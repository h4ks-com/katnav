# katnav

A web browser in your terminal. katnav embeds Chromium through CEF,
renders pages off screen, and shows their pixels through the kitty
graphics protocol, with [katui] drawing the frames. Pages keep everything
Chromium gives them: JavaScript, WebAssembly with threads, WebGL, WebGPU
and sound.

[katui]: https://github.com/h4ks-com/katui

## Build

You need Rust, [just](https://github.com/casey/just) and CMake. From a
checkout of this repository, this downloads CEF (about 350 MB) and builds
katnav with it:

```
just bundle
```

## Use

```
just run example.com
```

Scroll with the wheel, click, and type as in any browser. Press
<kbd>Ctrl</kbd>+<kbd>Q</kbd> to quit. Add `--game` to render games at a
lower resolution, and `--mute` to play no sound.

katnav looks best in kitty, Ghostty and WezTerm. Other terminals get sixel
images or colored blocks.

## Status

Early, and tested on macOS only. katnav opens one page and passes the
mouse and keyboard to it. There is no URL line, history or tabs yet.

## License

MIT or Apache-2.0, at your option. CEF and Chromium come under their own
BSD-style licenses.

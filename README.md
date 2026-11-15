# katnav

A web browser in your terminal. katnav runs Chrome headless and shows its
pixels through the kitty graphics protocol, with [katui] drawing the
frames. Pages keep everything Chrome gives them: JavaScript, WebAssembly
with threads, WebGL, WebGPU and sound.

[katui]: https://github.com/h4ks-com/katui

## Install

You need Rust and Google Chrome (Chromium, Edge or Brave work too). From a
checkout of this repository:

```
cargo install --path .
```

## Use

```
katnav example.com
```

Scroll with the wheel, click, and type as in any browser. Press
<kbd>Ctrl</kbd>+<kbd>Q</kbd> to quit. Set `KATNAV_CHROME` to the browser
binary when katnav does not find it.

katnav looks best in kitty, Ghostty and WezTerm. Other terminals get sixel
images or colored blocks.

## Status

Early. katnav opens one page and passes the mouse and keyboard to it.
There is no URL line, history or tabs yet.

## License

MIT or Apache-2.0, at your option.

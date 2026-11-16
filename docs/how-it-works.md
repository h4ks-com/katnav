# How katnav works

katnav embeds Chromium through CEF, the Chromium Embedded Framework, and
renders pages off screen. katui puts each frame on the terminal and brings
the terminal's input back to the page.

## Frames

CEF calls `OnPaint` with each frame as a BGRA buffer while katnav pumps
CEF's message loop from the katui event loop. katnav swaps it to RGBA and
hands it to katui's `PixelScreen` with the dirty rectangles CEF reports, so
katui compares only those tiles and sends only the ones that changed. A
still page costs nothing. On a local kitty or Ghostty, frames travel
through temporary files.

The device scale factor katnav reports to CEF sets the size of the paint
buffer while the page keeps its CSS layout. A governor picks that scale:
katui reports how long each frame took to reach the terminal, and when
frames take more than the frame budget, the governor lowers the scale.
Pages run between 0.25 and 1.0 of the terminal's pixels, half of that
under a multiplexer, and never paint more than 2 megapixels. The terminal
scales the picture up to fill its cells.

## Input

| Terminal (katui `InputEvent`) | Page (CEF) |
| --- | --- |
| `Key`, `KeyRepeat` | a raw key down with the Windows and native key codes, then a char event for text |
| `KeyUp` | key up |
| `Mouse` press, release | mouse click with the click count |
| back and forward buttons | back and forward in the page's history |
| `Mouse` move, drag | mouse move with the held buttons |
| `Mouse` wheel, up, down, left, right | mouse wheel, 100 a notch |
| `Paste` | committed IME text |
| `Focus(false)` | key up for every held key |

Chromium derives `KeyboardEvent.code` from the native key code, so
`src/keys.rs` maps each key to the macOS virtual key code and the X11 key
code of a US keyboard. On another layout the terminal also sends the base
layout key, and katnav takes the physical key from it. Super is the
command modifier. On a terminal that does not report key releases, katnav
releases every key right after its press, so no key sticks.

When the page changes the pointer, katnav sets the terminal's pointer
shape to match.

## Game controllers

Chromium reads controllers only for the app in front, which is the
terminal. katui's `gamepad` module reads them instead, and katnav gives
the page a `navigator.getGamepads` that returns them in the standard
layout, so web games see them as usual. katnav keeps only each
controller's newest state and hands it to the page once per loop.

## Profile

The profile in `~/.local/share/katnav/profile` keeps cookies and storage.
Chromium hands a URL to whichever process holds a profile, so a second
katnav running at the same time takes a temporary profile of its own.

## Layout

CEF needs its files beside katnav: an app bundle with helper apps on
macOS, the libraries and resources next to the binary on Linux and
Windows. `just bundle` builds that layout. CEF's sandbox is off. On Linux katnav
runs Chromium on its headless platform, so it needs no X server or
Wayland, and asks for WebGPU over Vulkan with `--enable-unsafe-webgpu` and
`--enable-features=Vulkan`.

## Tracing

`KATNAV_TRACE=trace.txt just run <url>` writes a line for each frame (its
encoding and delivery time, whether the terminal answered, and how old
CEF's frame was), each slow CEF pump, each key and each controller
update.

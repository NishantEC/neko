# neko.icns

The app icon: a black cat peeking over a ledge, on a cream ground.

**Nothing consumes this yet, and that is expected.** neko builds as a bare
Mach-O rather than a `.app` bundle — the same fact that puts `SMAppService`
and notification banners out of reach (see `AGENTS.md`). A `.icns` has
nothing to attach to until the app is packaged. It is committed now so that
day is a one-line change rather than a design exercise.

When bundling, it goes in `Neko.app/Contents/Resources/` with
`CFBundleIconFile` naming it in `Info.plist`.

## This is not the same artwork as `mark.svg`

| | where | why |
| --- | --- | --- |
| `crates/neko/assets/icons/neko/mark.svg` | menu bar, onboarding header | single tone — `gpui::svg()` renders an **alpha mask**, so colour is impossible |
| `packaging/icon/neko.icns` | the app icon | a real image file, no such limit |

Both are the same animal; they are deliberately not the same drawing. That
split is ordinary macOS practice: a colourful app icon, a template glyph in
the menu bar.

## Regenerating

`generate.py` emits one SVG per size. It is **seven drawings, not one scaled**
— detail is dropped as the pixels run out:

| pixels | treatment |
| --- | --- |
| 256–1024 | full: rim light, gloss, muzzle lift, nose, mouth, two eye highlights |
| 64–128 | second eye highlight dropped, eyes enlarged |
| 16–32 | nose, mouth and rim dropped, eyes enlarged again |

**The face is round-pupilled on purpose.** An earlier pass used vertical slit
pupils and read as predatory rather than approachable — a slit is the single
strongest "this animal hunts" cue available. Round pupils, two highlights, a
pink nose and a small mouth are what make it friendly, and they cost nothing
above 64px. Below that the mouth and nose are the first to go: they collapse
into one dark smudge and dirty the muzzle.

```sh
python3 generate.py                      # writes n<size>.svg
for s in 1024 512 256 128 64 32 16; do   # rasterise
  qlmanage -t -s $s -o . n$s.svg && mv n$s.svg.png r$s.png
done
# then mask each r<size>.png through a 22.37% rounded-rect alpha and assemble
iconutil -c icns neko.iconset -o neko.icns
```

**The alpha mask is not optional.** `qlmanage` flattens SVG onto an *opaque*
canvas, so the squircle `clipPath` inside the file buys nothing — without
masking, the corners ship opaque white and the Dock shows a hard square.

## Verifying

Read the file back rather than trusting the build:

```sh
iconutil -c iconset neko.icns -o verify.iconset
```

Then check alpha at pixel `(0, 0)` (must be 0) and at centre (must be 255) for
all ten slots. **Probe the true corner, not an inset one** — at 16px the corner
radius is only 3.6px, so `(2, 2)` sits *inside* the shape and reports a false
failure.

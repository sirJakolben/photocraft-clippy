# Clippy app icon

`clippy.svg` is the source: a white brush stroke curling into a "C", ending in a yellow pen nib, on
a teal-to-indigo rounded square. The PNGs under `hicolor/` are renders of it:

```sh
for s in 16 24 32 48 64 128 256 512; do
  magick -background none -density 384 clippy.svg -resize ${s}x${s} -depth 8 \
    PNG32:hicolor/${s}x${s}/apps/io.github.sirjakolben.PhotocraftClippy.png
done
```

Placeholder until a final icon is drawn. It deliberately avoids a paperclip, so it is not mistaken
for Microsoft's Office assistant.

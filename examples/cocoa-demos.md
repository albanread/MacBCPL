# Cocoa demos

These programs drive macOS Cocoa entirely through Objective-C **bracket
message sends** (`[receiver selector: arg …]`) compiled from BCPL. They
exercise the full object stack: BCPL strings are `NSString`s, return types
are synthesized from the selector database, struct returns become BCPL
vectors, and any Cocoa class is reachable as a message receiver.

Run with:

```
./target/debug/newbcpl-driver run examples/<name>.bcl
```

| Demo | What it shows | Needs a desktop? |
|------|---------------|------------------|
| `cocoa-sysinfo.bcl` | `NSProcessInfo` / `NSFileManager` queries; chained `NSString` methods; synthesized `String`/`INT` returns | no (console) |
| `cocoa-collections.bcl` | `NSMutableArray` / `NSMutableDictionary` build + query; `componentsJoinedByString:` | no (console) |
| `cocoa-geometry.bcl` | struct returns → vectors: `NSRange` (int-pair), `NSSize` (HFA), `NSRect` (sret), read by field name | no (console) |
| `cocoa-alert.bcl` | a native `NSAlert` dialog | yes |
| `cocoa-window.bcl` | a native `NSWindow` + AppKit run loop | yes |

## Notes

- **Return types are synthesized** from `cocoa-selectors.json`: `[arr count]`
  is an `INT`, `[s uppercaseString]` is a `String`, `[view bounds]` is an
  `NSRect` vector — no annotations needed. Use a trailing `AS Type`
  (`[obj thing] AS INT`) to override or for selectors the DB doesn't cover.
- **Struct returns** arrive as a vector: read `NSRect`/`NSPoint`/`NSSize`
  fields with the float subscript `.%` and `NSRange` with `!`, using the
  seeded field names (`NSRect_width`, `NSRange_location`, …).
- **Sizing a window** (and other struct-valued *arguments* like
  `setFrame:` / `setContentSize:`) is the next bracket-send increment;
  `cocoa-window.bcl` uses the default size and `center` (no struct arg).
- The console demos run anywhere; the UI demos need a desktop session to
  display (headless, the event loop / `runModal` return immediately).

## Whole-ecosystem type synthesis (`COCOA_SQLITE`)

By default the compiler synthesizes return/argument types from a bundled
selector table covering ~40 common classes. Point it at the shared
[`cocoa_data`](../../cocoa_data) SQLite mirror to cover the **entire**
Obj-C surface (26k classes, every method's encoding), class-aware:

```sh
COCOA_SQLITE=/path/to/cocoa_data/cocoa.sqlite ./newbcpl-driver run prog.bcl
```

With it set, selectors on any class — e.g. `[[NSProcessInfo processInfo]
activeProcessorCount]` — get their types (Int/Float/struct) without an
`AS Type` annotation. Without it, the bundled table is used and unknown
selectors default to `id` (annotate with `AS Type` as needed). Signatures
are derived on demand by parsing the runtime `@encode`, so it's a drop-in
upgrade — no rebuild, just the env var.

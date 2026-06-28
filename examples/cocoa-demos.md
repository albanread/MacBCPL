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

# Third-party notices and provenance

## TaskbarX by Chris Andriessen

- Upstream: [ChrisAnd1998/TaskbarX](https://github.com/ChrisAnd1998/TaskbarX).
- Copyright (c) 2021 Chris Andriessen.
- Source license: MIT. The complete upstream license is preserved in this repository's [LICENSE](LICENSE) and must accompany the portable package.
- This is an independent Rust rewrite, not an official upstream release or endorsement.

[The parity inventory](docs/PARITY.md) records the seven fully reviewed local source files, line counts and SHA-256 hashes: Settings.vb, Main.vb, TaskbarStyle.vb, TaskbarCenter.vb, TrayIconBuster.vb, MainWindow.xaml and MainWindow.xaml.vb. Related geometry/animation reference includes TaskbarAnimate.vb and Easings.vb. Preserve attribution for derived source/formulas.

Review does not imply copying every behavior. Process-name killing, upstream task migration, Store activation/uninstall and phantom-tray remote-memory cleanup are deliberately excluded. The hidden/disabled/dormant upstream controls are documented without representing them as working features.

## Excluded upstream material

No upstream executable is a Rust build output. Do not redistribute or rename upstream EXEs/DLLs as the rewrite. No signing keys, PFX files, certificates or Store packaging credentials are copied or used. A project-file reference to a signing key is not permission to use it.

Do not assume bundled icons, screenshots, fonts, installers, compiled libraries or Store assets share the source license. The release procedure does not need these assets. Any future inclusion requires separate provenance/license review. User-supplied shortcut icons are not upstream bundled assets.

The native tray loads Windows' shared `IDI_APPLICATION` icon and owns a `CopyIcon` copy for cleanup. It does not require a redistributed or runtime-loaded icon asset. Optional shortcut icons remain user-supplied; their availability, format and licensing are the user's responsibility.

Upstream references ModernWPF, TaskScheduler and Emoji.Wpf; this does **not** declare those .NET libraries to be Rust dependencies. Their licenses cannot be inferred from TaskbarX's MIT license. Upstream VERSION/release checks describe upstream TaskbarX, not releases of this independent product.

## Rust dependency review

Review the **actual integrated Cargo.lock and Cargo metadata**, including `taskbarx-shell`, before external distribution. The release procedure records resolved package names, versions, sources and declared license expressions in `evidence/dependency-inventory.json`, alongside Cargo metadata.

That file is an **inventory, not a license-compliance certification**. Declared expressions alone do not replace required copyright/license texts. Missing/ambiguous metadata, vendored source, fonts and resources need manual review. Add required supplemental notices/texts under `docs/licenses/`; `docs/` is included in the portable package. Rebuild/package after notice changes.

A local validation ZIP is not a claim that redistribution obligations have been fully reviewed or permission to publish. The scripts do not sign, upload or publish.

## Private preview artwork and dependency notices

See [artwork provenance](docs/assets/README.md) for the upstream banner, its edited Rust-badge variant, and the CC-BY-4.0 Rust logo reference, and [dependency license texts](docs/licenses/README.md) for the locked Rust dependencies. The banner depicts upstream, not this build; review its separate artwork rights before making the repository public. Optional upstream .ico downloads are linked rather than bundled.


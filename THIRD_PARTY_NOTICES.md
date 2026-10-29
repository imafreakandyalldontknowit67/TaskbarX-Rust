# Third-party notices and provenance

## TaskbarX by Chris Andriessen

- Upstream project: [ChrisAnd1998/TaskbarX](https://github.com/ChrisAnd1998/TaskbarX).
- Copyright (c) 2021 Chris Andriessen.
- Source license: MIT. The complete upstream `LICENSE` is copied **verbatim** into this repository's `LICENSE` and must accompany the portable package.
- This project is an independent Rust rewrite, not an official upstream release or an endorsement by the upstream author.

The local reference reviewed for the rewrite comprises upstream `README.md`, `TaskbarX/TaskbarX/TaskbarX.vbproj`, `Main.vb`, `Settings.vb`, `TaskbarCenter.vb`, `TaskbarStyle.vb`, `Easings.vb`, and `TaskbarX Configurator/TaskbarX Configurator/MainWindow.xaml.vb`. It informs geometry/easing behavior, shell discovery, appearance, configuration and explicit differences from legacy behavior. Preserve upstream attribution for derived source or formulas.

## Excluded upstream material

No upstream executable is a Rust build output. Do not redistribute or rename upstream EXEs/DLLs as the rewrite, and do not copy signing keys, PFX files, certificates or Store packaging credentials. The upstream project file references a signing-key file; that is not permission to use it.

Do not assume bundled icons, screenshots, installers, compiled libraries, Store assets or other binary assets have the same license as upstream source. None is required by these packaging scripts. Adding any such asset requires separate provenance and license review first.

Upstream's README mentions ModernWPF, TaskScheduler and Emoji.Wpf. These are upstream references, **not a declaration that this Rust workspace includes or redistributes them**. Their licenses must not be inferred from TaskbarX's MIT license.

## Rust dependency inventory: integration review required

The documentation worktree has no combined Cargo dependency graph or lockfile. A complete third-party dependency notice list cannot honestly be asserted here. During integration, review the actual `Cargo.lock`, Cargo metadata and every distributed dependency's license texts/notice obligations. Update this file (and add required license texts under `docs/licenses/`) before distributing a package outside the local validation workflow.

`release.ps1` records the resolved package names, versions, source identifiers and declared license expressions in `evidence/dependency-inventory.json`, alongside the complete Cargo metadata. This is an **inventory, not a license-compliance certification**. License expressions alone do not replace required license texts. Missing/ambiguous license metadata, vendored code, fonts or other resources require manual review. The local ZIP is a validation candidate, not permission to publish.

The portable package includes `docs/` so reviewed supplemental notices placed there travel with it. No signing, upload or publication is performed by the scripts.

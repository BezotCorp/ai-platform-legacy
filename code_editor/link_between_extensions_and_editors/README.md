# Link Between Extensions And Editors

Boundary between generic extensions and editor-specific integrations.

This folder owns the common editor-operation vocabulary, such as `current_file`, `current_selection`, `open_file`, and `apply_edit`.

It must not become a mandatory Rust library. If a concrete component is needed here later, it should be introduced as its own explicitly scoped executable or transport implementation.

qt-build-utils 0.7.3 from crates.io, with one change in src/lib.rs
(qrc_list): `.split('\n')` is now `.lines()`. The split left a trailing empty
entry, which cxx-qt-build printed as an empty `rerun-if-changed` path; Cargo
reads that as the whole crate folder, so the C++ build reran on every build.
Fixed upstream in later versions; drop this when moving to cxx-qt 0.8+.

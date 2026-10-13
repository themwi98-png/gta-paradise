Vendored rerecast 0.4.0 (https://crates.io/crates/rerecast, MIT OR Apache-2.0).

Local change (src/poly_mesh.rs, polygon mesh building): a contour that fails to
triangulate is skipped, as C++ Recast does, instead of aborting the whole mesh.

// Compatibility symbol for arborium-tree-sitter on wasm32-unknown-unknown.
// Allocation failure is terminal in tree-sitter, so no writable FILE is needed.
void *stderr = 0;

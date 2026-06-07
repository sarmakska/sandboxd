;; A module that uses the audited host::random capability.
;;
;; It imports host::random, a seeded deterministic 64-bit generator, and an
;; export `roll` that returns the next value truncated to i32. When the embedder
;; grants the capability with a seed the result is reproducible; when it does
;; not, instantiation fails with SandboxError::DisallowedImport. The module
;; needs no linear memory: host::random hands back a number directly.
(module
  (import "host" "random" (func $random (result i64)))
  (func (export "roll") (result i32)
    (i32.wrap_i64 (call $random))))

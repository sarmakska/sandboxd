;; A module that uses the audited host::log capability.
;;
;; It stores the bytes "hello from the guest" into linear memory at offset 0
;; and calls host::log with that pointer and length. When the embedder grants
;; the log capability the line is captured; when it does not, instantiation
;; fails with SandboxError::DisallowedImport. The guest exports its memory as
;; `memory` so the host can read the string safely with bounds checking.
(module
  (import "host" "log" (func $log (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "hello from the guest")
  (func (export "run")
    (call $log (i32.const 0) (i32.const 20))))

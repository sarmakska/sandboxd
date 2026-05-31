;; A module that imports a host function that is not on the allow-list.
;;
;; `env::secret` is not a capability sandboxd grants, so instantiation fails
;; with SandboxError::DisallowedImport before any guest code runs. This is the
;; deny-by-default contract: the guest cannot reach anything the embedder did
;; not explicitly permit.
(module
  (import "env" "secret" (func $secret (result i32)))
  (func (export "run") (result i32)
    call $secret))

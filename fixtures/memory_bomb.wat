;; A module that tries to grow its linear memory without bound.
;;
;; It starts with a single page and repeatedly calls memory.grow by 16 pages
;; (1 MiB) at a time. Under the memory cap, memory.grow returns -1 once the
;; cap is reached. The module checks for that and executes `unreachable`,
;; which sandboxd reports as SandboxError::MemoryLimitExceeded.
(module
  (memory (export "memory") 1)
  (func (export "run")
    (loop $grow
      ;; grow by 16 pages; result is the previous page count, or -1 on failure
      (i32.const 16)
      memory.grow
      (i32.const -1)
      i32.eq
      (if
        (then unreachable))
      br $grow)))

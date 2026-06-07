;; A module that grows its linear memory by a bounded amount and stops.
;;
;; It starts with a single page (64 KiB) and grows by 31 more pages, reaching
;; exactly 32 pages (2 MiB), then returns the final page count. Under a memory
;; cap above 2 MiB the growth is allowed and the run reports a peak linear
;; memory of 2 MiB. This is the counterpart to memory_bomb.wat: a guest that
;; allocates, but stays within its budget.
(module
  (memory (export "memory") 1)
  (func (export "run") (result i32)
    (drop (memory.grow (i32.const 31)))
    (memory.size)))

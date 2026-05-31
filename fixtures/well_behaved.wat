;; A well-behaved, pure module.
;;
;; `add` returns the sum of its two i32 parameters. `fib` computes a Fibonacci
;; number iteratively. Neither imports anything, neither allocates unbounded
;; memory, and both terminate quickly, so they run to completion under any
;; sane limit set and return deterministic values.
(module
  (func (export "add") (param i32 i32) (result i32)
    local.get 0
    local.get 1
    i32.add)

  (func (export "fib") (param $n i32) (result i32)
    (local $a i32)
    (local $b i32)
    (local $i i32)
    (local $tmp i32)
    (local.set $a (i32.const 0))
    (local.set $b (i32.const 1))
    (local.set $i (i32.const 0))
    (block $done
      (loop $next
        (br_if $done (i32.ge_s (local.get $i) (local.get $n)))
        (local.set $tmp (i32.add (local.get $a) (local.get $b)))
        (local.set $a (local.get $b))
        (local.set $b (local.get $tmp))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $next)))
    local.get $a))

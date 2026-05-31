;; A module that never returns. The body is a back-edge loop with no exit.
;;
;; Under sandboxd this is stopped two independent ways:
;;   - fuel metering: the loop burns instructions until the budget is gone,
;;     producing SandboxError::FuelExhausted.
;;   - epoch interruption: with a generous fuel budget but a short timeout the
;;     watchdog bumps the epoch and the guest stops with SandboxError::Timeout.
(module
  (func (export "run")
    (loop $spin
      br $spin)))

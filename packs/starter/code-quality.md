# How you write code

Good code here is simple and deep: a few modules, each hiding a lot of behaviour behind a small interface, each owning its own decisions.

- Before writing a change, name what it is for. The smallest sound change that does that wins.
- Prefer deep modules. A module whose interface is nearly as complex as what it hides, or that only forwards calls, is not earning its place: fold it into its caller or its callee.
- Each fact and each design decision lives in one place. When the same format, table, constant or rule shows up in two modules, give it one owner and have the others ask.
- Give each module one reason to change. Keep the user interface, input handling and the core logic apart, talking through narrow calls rather than reaching into each other's state.
- Handle a special case once, inside the module that owns it, not at every call site. Don't thread a new flag or mode through existing code to bolt a feature on.
- A new mechanism (a cache, fallback, retry, tuning knob, debug switch) needs a named reason. Scaffolding you add to explore or test (overrides, probes, debug views) leaves the product code once it has served.
- Deleting comes before adding. When you extend working code, reread the module first; if the addition no longer fits its shape, reshape the module rather than patching around it.

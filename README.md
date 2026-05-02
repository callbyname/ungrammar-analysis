# ungrammar-analysis

Convenience queries for `ungrammar`.

`ungrammar` is deliberately small and does not expose an API for extracting higher-level
structural information.

This crate provides `Analysis`, a companion view for name lookup (`Node`
and `Token`) and rule characterisation (`Symbol` to `Cardinality`).

```rust
use std::collections::HashMap;
use ungrammar_analysis::{Analysis, Cardinality, Symbol};

let grammar = "
    A = B 'c' | 'c'
    B = 'b'
"
.parse()
.unwrap();

let analysis = Analysis::from(&grammar);

let node_a = analysis.node("A").unwrap();
let node_b = analysis.node("B").unwrap();
let token_c = analysis.token("c").unwrap();

let symbols = analysis
    .rule_analysis(node_a)
    .unwrap()
    .symbols()
    .collect::<HashMap<_, _>>();

assert_eq!(symbols[&node_b.into()], Cardinality::Optional);
assert_eq!(symbols[&token_c.into()], Cardinality::One);
```




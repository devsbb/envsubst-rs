use std::io::{BufReader, Cursor};

use criterion::{criterion_group, criterion_main, Criterion};
use envsubst::Parser;

const TEMPLATE: &str = r#"${PATH}
${PWD}
$BROWSER
$PATH
$PWD
"#;

fn criterion_benchmark(c: &mut Criterion) {
    let huge_template = (0..10_000)
        .map(|_| TEMPLATE.to_owned())
        .collect::<Vec<String>>();
    let huge_template = huge_template.join("\n");
    c.bench_function("render", |b| {
        b.iter(|| {
            let input = BufReader::new(Cursor::new(&huge_template));
            let output = Cursor::new(vec![]);
            let mut s = Parser::new(input, output, true);
            s.process().unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);

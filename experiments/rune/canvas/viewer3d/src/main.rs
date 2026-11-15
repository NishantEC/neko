fn main() { rune::run(rune::AppSpec::new("x", (200.0, 200.0), || (), |_: &(), _: &mut rune::Cx<()>| rune::col())); }

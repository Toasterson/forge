use component::Component;

fn main() -> miette::Result<()> {
    let test_component = Component::open_local("sample_data/openjdk11")?;

    println!("{:#?}", test_component);
    Ok(())
}

use integration::emit_schema;
use miette::Result;

fn main() -> Result<()> {
    let schema = emit_schema()?;
    println!("{}", schema);
    Ok(())
}

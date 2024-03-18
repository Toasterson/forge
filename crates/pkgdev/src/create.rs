use component::Component;

use crate::args::ComponentArgs;

pub(crate) fn create_component(
    arg: ComponentArgs,
    fmri: String,
) -> miette::Result<()> {
    let c = Component::new(fmri, Some(arg.component))?;
    c.save_document()?;
    Ok(())
}

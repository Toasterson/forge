use crate::Result;
use component::Component;
use gate::Gate;

pub(crate) fn open_component_local<P: AsRef<std::path::Path>>(component_path: P, gate: &Option<Gate>) -> Result<Component> {
    let component_path = component_path.as_ref();
    let full_component_path = if let Some(gate) = gate {
        // If we have a gate we look for the component under <gate_path>/components/<component_path>
        gate.get_gate_path().join("components").join(component_path)
    } else {
        component_path.to_path_buf()
    };

    let full_component_path = full_component_path.canonicalize()?;

    Ok(Component::open_local(full_component_path.as_path())?)
}
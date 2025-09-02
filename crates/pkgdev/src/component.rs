use component::Component;
use gate::Gate;
use miette::{IntoDiagnostic, WrapErr};
use std::path::{Component as PathComponent, Path};

fn first_segment_is_components<P: AsRef<Path>>(p: P) -> bool {
    p.as_ref()
        .components()
        .next()
        .map(|c| matches!(c, PathComponent::Normal(os) if os == "components"))
        .unwrap_or(false)
}

fn cwd_is_inside_components() -> miette::Result<bool> {
    let cwd = std::env::current_dir().into_diagnostic()?;
    Ok(cwd
        .components()
        .any(|c| matches!(c, PathComponent::Normal(os) if os == "components")))
}

pub(crate) fn open_component_local<P: AsRef<std::path::Path>>(
    component_path: P,
    gate: &Option<Gate>,
) -> miette::Result<Component> {
    let component_path = component_path.as_ref();

    // If an absolute path is provided, honor it regardless of gate.
    let full_component_path = if component_path.is_absolute() {
        component_path.to_path_buf()
    } else if let Some(gate) = gate {
        // If a gate is provided, prefer the current working directory if it points to a component.
        // This allows `--component .` to work when invoked from inside a component directory.
        let cwd = std::env::current_dir().into_diagnostic()?;
        let cwd_candidate = cwd.join(component_path);
        let cwd_package = cwd_candidate.join("package.kdl");
        if cwd_package.exists() {
            cwd_candidate
        } else {
            // Fall back to <gate_path>/components/<component_path>
            let base = gate.get_gate_path();
            if first_segment_is_components(component_path) {
                base.join(component_path)
            } else {
                base.join("components").join(component_path)
            }
        }
    } else {
        // No gate provided: treat the current working directory as the gate root
        // If we're already inside a components/ subtree, don't prefix again.
        let cwd = std::env::current_dir().into_diagnostic()?;
        if first_segment_is_components(component_path) || cwd_is_inside_components()? {
            cwd.join(component_path)
        } else {
            cwd.join("components").join(component_path)
        }
    };

    let full_component_path = full_component_path
        .canonicalize()
        .into_diagnostic()
        .wrap_err_with(|| format!(
            "failed to resolve component path (canonicalize) for '{}'. Hint: ensure the component directory exists under <gate>/components/<component> or provide an absolute path.",
            component_path.display()
        ))?;

    Ok(Component::open_local(full_component_path.as_path())
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to open component at '{}': missing package.kdl or invalid component layout",
                full_component_path.display()
            )
        })?)
}

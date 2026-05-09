use fabro_types::settings::ProjectNamespace;

use super::ResolveError;
use crate::ProjectLayer;

pub fn resolve_project(layer: &ProjectLayer, _errors: &mut Vec<ResolveError>) -> ProjectNamespace {
    ProjectNamespace {
        name:        layer.name.clone(),
        description: layer.description.clone(),
        metadata:    layer.metadata.clone().into_inner(),
    }
}

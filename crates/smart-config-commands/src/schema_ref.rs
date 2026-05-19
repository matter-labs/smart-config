use smart_config::{ConfigRef, metadata::ConfigVariant};

use crate::ParamRef;

pub(crate) fn collect_conditions(mut config: ConfigRef<'_>) -> Vec<(ParamRef<'_>, &ConfigVariant)> {
    let mut conditions = vec![];
    while let Some((parent_ref, this_ref)) = config.parent_link() {
        if let Some(variant) = this_ref.tag_variant {
            conditions.push((ParamRef::for_tag(parent_ref), variant));
        }
        config = parent_ref;
    }
    conditions
}

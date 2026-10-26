//! Id generation. Core treats ids as opaque UUID strings; this is the one place that mints them.

use crate::model::Id;

/// A fresh random (v4) UUID string, e.g. `"f47ac10b-58cc-4372-a567-0e02b2c3d479"`.
pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

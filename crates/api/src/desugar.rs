//! Facade-Substrate Desugaring
//!
//! This module documents how every Facade API call desugars to Substrate API calls.
//! This is the canonical reference for understanding the implicit behaviors of the Facade.
//!
//! ## Design Principle
//!
//! **Every facade call desugars to exactly one substrate call pattern.**
//!
//! There is no magic, no hidden semantics, no side effects beyond what is documented here.
//!
//! ## Common Implicit Behaviors
//!
//! All facade operations share these implicit behaviors:
//!
//! 1. **Default Run**: Operations target `ApiRunId::default_run_id()` ("default")
//! 2. **Auto-Commit**: Each operation auto-commits (unless in a batch)
//! 3. **Version Stripping**: Return values omit version info by default
//! 4. **Error Mapping**: Substrate errors pass through unchanged
//!
//! ## Desugaring Tables
//!
//! ### KV Facade
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `get(key)` | `kv_get(default, key).map(\|v\| v.map(\|x\| x.value))` |
//! | `get_with_options(key, opts)` | `kv_get(default, key).map(\|v\| v.map(\|x\| (x.value, x.version)))` or `kv_get_at(...)` |
//! | `set(key, value)` | `kv_put(default, key, value); ()` |
//! | `set_with_options(key, value, opts)` | See detailed section below |
//! | `del(key)` | `kv_delete(default, key)` |
//! | `exists(key)` | `kv_exists(default, key)` |
//! | `incr(key)` | `kv_incr(default, key, 1)` |
//! | `incrby(key, delta)` | `kv_incr(default, key, delta)` |
//! | `decr(key)` | `kv_incr(default, key, -1)` |
//! | `decrby(key, delta)` | `kv_incr(default, key, -delta)` |
//! | `setnx(key, value)` | `kv_cas_version(default, key, None, value)` |
//! | `getset(key, value)` | `kv_get(default, key).map(\|v\| v.map(\|x\| x.value)); kv_put(default, key, value)` |
//! | `mget(keys)` | `kv_mget(default, keys).map(\|vs\| vs.map(\|v\| v.map(\|x\| x.value)))` |
//! | `mset(entries)` | `kv_mput(default, entries); ()` |
//! | `mdel(keys)` | `kv_mdelete(default, keys)` |
//! | `mexists(keys)` | `kv_mexists(default, keys)` |
//!
//! #### Set With Options Details
//!
//! ```text
//! set_with_options(key, value, opts):
//!   if opts.only_if_not_exists (NX):
//!     success = kv_cas_version(default, key, None, value)
//!     return if opts.get_old_value { None } else { () }
//!
//!   if opts.only_if_exists (XX):
//!     existing = kv_get(default, key)
//!     if existing.is_none():
//!       return None  // or error
//!     kv_put(default, key, value)
//!     return if opts.get_old_value { existing.map(|v| v.value) } else { () }
//!
//!   if opts.expected_version:
//!     success = kv_cas_version(default, key, opts.expected_version, value)
//!     return if success { Some(()) } else { None }
//!
//!   // Default: unconditional set
//!   if opts.get_old_value:
//!     old = kv_get(default, key).map(|v| v.value)
//!     kv_put(default, key, value)
//!     return old
//!   else:
//!     kv_put(default, key, value)
//!     return None
//! ```
//!
//! ### JSON Facade
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `json_get(key, path)` | `json_get(default, key, path).map(\|v\| v.map(\|x\| x.value))` |
//! | `json_set(key, path, value)` | `json_set(default, key, path, value); ()` |
//! | `json_del(key, path)` | `json_delete(default, key, path)` |
//! | `json_merge(key, path, patch)` | `json_merge(default, key, path, patch); ()` |
//! | `json_type(key, path)` | `json_get(default, key, path).map(\|v\| v.map(\|x\| type_name(x.value)))` |
//! | `json_numincrby(key, path, delta)` | Read + modify + write pattern |
//! | `json_strappend(key, path, suffix)` | Read + modify + write pattern |
//! | `json_arrappend(key, path, values)` | `json_set(default, key, path+"[-]", value)` for each |
//! | `json_arrlen(key, path)` | `json_get(default, key, path).map(\|v\| v.array().len())` |
//! | `json_objkeys(key, path)` | `json_get(default, key, path).map(\|v\| v.object().keys())` |
//! | `json_objlen(key, path)` | `json_get(default, key, path).map(\|v\| v.object().len())` |
//!
//! ### Event Facade
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `xadd(stream, payload)` | `event_append(default, stream, payload).as_u64()` |
//! | `xrange(stream, start, end)` | `event_range(default, stream, start, end, None)` |
//! | `xrange_count(stream, start, end, count)` | `event_range(default, stream, start, end, Some(count))` |
//! | `xrevrange(stream, start, end)` | `event_range(default, stream, end, start, None).reverse()` |
//! | `xlen(stream)` | `event_len(default, stream)` |
//! | `xlast(stream)` | `event_latest_sequence(default, stream)` |
//! | `xget(stream, sequence)` | `event_get(default, stream, sequence)` |
//!
//! ### State Facade
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `state_get(cell)` | `state_get(default, cell).map(\|v\| StateValue{...})` |
//! | `state_set(cell, value)` | `state_set(default, cell, value).as_u64()` |
//! | `state_cas(cell, expected, value)` | `state_cas(default, cell, expected, value).map(\|v\| v.as_u64())` |
//! | `state_del(cell)` | `state_delete(default, cell)` |
//! | `state_exists(cell)` | `state_exists(default, cell)` |
//!
//! ### Vector Facade
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `vadd(coll, key, vec, meta)` | `vector_upsert(default, coll, key, vec, meta); ()` |
//! | `vget(coll, key)` | `vector_get(default, coll, key).map(\|v\| v.value)` |
//! | `vdel(coll, key)` | `vector_delete(default, coll, key)` |
//! | `vsim(coll, query, k)` | `vector_search(default, coll, query, k, None, None)` |
//! | `vsim_with_options(...)` | `vector_search(default, coll, query, k, filter, metric)` |
//! | `vcollection_info(coll)` | `vector_collection_info(default, coll)` |
//! | `vcollection_drop(coll)` | `vector_drop_collection(default, coll)` |
//!
//! ## Type Conversions
//!
//! ### Version Stripping
//!
//! ```text
//! // Substrate returns
//! Versioned<Value> { value, version, timestamp }
//!
//! // Facade returns (by default)
//! value  // version and timestamp stripped
//! ```
//!
//! ### Error Passthrough
//!
//! All substrate errors pass through unchanged:
//! - `NotFound` → `NotFound`
//! - `WrongType` → `WrongType`
//! - `InvalidKey` → `InvalidKey`
//! - etc.
//!
//! ## Implementation Reference
//!
//! A conforming facade implementation MUST implement these exact semantics.
//! The desugaring is the specification - implementations may optimize
//! as long as observable behavior matches.

use crate::substrate::ApiRunId;

/// The default run used by all facade operations
///
/// All facade operations implicitly target this run.
#[inline]
pub fn default_run() -> ApiRunId {
    ApiRunId::default_run_id()
}

/// Documentation marker trait for desugaring
///
/// Types implementing this trait have documented desugaring semantics.
pub trait HasDesugaring {
    /// The substrate trait this desugars to
    type SubstrateTrait: ?Sized;
}

// KV Facade desugaring marker
impl crate::facade::kv::KVFacade for () {
    fn get(&self, _: &str) -> strata_core::StrataResult<Option<strata_core::Value>> {
        unimplemented!("This is a documentation marker only")
    }
    fn getv(&self, _: &str) -> strata_core::StrataResult<Option<crate::facade::kv::Versioned<strata_core::Value>>> {
        unimplemented!("This is a documentation marker only")
    }
    fn get_with_options(&self, _: &str, _: crate::facade::types::GetOptions)
        -> strata_core::StrataResult<Option<(strata_core::Value, Option<u64>)>> {
        unimplemented!("This is a documentation marker only")
    }
    fn set(&self, _: &str, _: strata_core::Value) -> strata_core::StrataResult<()> {
        unimplemented!("This is a documentation marker only")
    }
    fn set_with_options(&self, _: &str, _: strata_core::Value, _: crate::facade::types::SetOptions)
        -> strata_core::StrataResult<Option<strata_core::Value>> {
        unimplemented!("This is a documentation marker only")
    }
    fn del(&self, _: &str) -> strata_core::StrataResult<bool> {
        unimplemented!("This is a documentation marker only")
    }
    fn exists(&self, _: &str) -> strata_core::StrataResult<bool> {
        unimplemented!("This is a documentation marker only")
    }
    fn incr(&self, _: &str) -> strata_core::StrataResult<i64> {
        unimplemented!("This is a documentation marker only")
    }
    fn incrby(&self, _: &str, _: i64) -> strata_core::StrataResult<i64> {
        unimplemented!("This is a documentation marker only")
    }
    fn incr_with_options(&self, _: &str, _: i64, _: crate::facade::types::IncrOptions)
        -> strata_core::StrataResult<i64> {
        unimplemented!("This is a documentation marker only")
    }
    fn setnx(&self, _: &str, _: strata_core::Value) -> strata_core::StrataResult<bool> {
        unimplemented!("This is a documentation marker only")
    }
    fn getset(&self, _: &str, _: strata_core::Value)
        -> strata_core::StrataResult<Option<strata_core::Value>> {
        unimplemented!("This is a documentation marker only")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_run() {
        let run = default_run();
        assert!(run.is_default());
        assert_eq!(run.as_str(), "default");
    }
}

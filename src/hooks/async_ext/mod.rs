use std::{future::Future, pin::Pin};

use super::{Hook, HookOps};

pub type AsyncZholHook = std::sync::Arc<dyn AsyncHookOps>;

#[cfg(feature = "async")]
pub trait AsyncHookOps: HookOps {
    fn async_unhook(
        &self,
        timeout: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = crate::MemOpResult<()>> + Send + '_>>;
    fn async_hook(
        &mut self,
        timeout: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = crate::MemOpResult<()>> + Send + '_>>;
}

pub fn to_hook_ops(async_hook: &std::sync::Arc<dyn AsyncHookOps>) -> std::sync::Arc<dyn HookOps> {
    // SAFETY: AsyncHookOps is a supertrait of HookOps, so this conversion is safe.
    // The vtable for AsyncHookOps contains all the HookOps methods at compatible offsets.
    unsafe { std::mem::transmute(async_hook.to_owned()) }
}

#[cfg(feature = "async")]
impl AsyncHookOps for Hook {
    #[cfg(feature = "async")]
    fn async_unhook(
        &self,
        timeout: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = crate::MemOpResult<()>> + Send + '_>> {
        use crate::hooks::{Hook, HookOps};
        use crate::{await_memop, MemOpResult};
        Box::pin(async move {
            await_memop!(&self.clone(), |h: Hook| -> MemOpResult<()> {
                h.unhook(timeout)
            })
        })
    }

    #[cfg(feature = "async")]
    fn async_hook(
        &mut self,
        timeout: std::time::Duration,
    ) -> Pin<Box<dyn Future<Output = crate::MemOpResult<()>> + Send + '_>> {
        use crate::hooks::{Hook, HookOps};
        use crate::{await_memop, MemOpResult};
        Box::pin(async move {
            await_memop!(&self.clone(), |h: Hook| -> MemOpResult<()> {
                h.hook(timeout)
            })
        })
    }
}

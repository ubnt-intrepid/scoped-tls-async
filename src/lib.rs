use futures_core::{
    future::Future,
    task::{self, Poll},
};
use std::pin::Pin;

#[doc(no_inline)]
pub use scoped_tls::{scoped_thread_local, ScopedKey};

pub trait ScopedKeyExt<T> {
    fn set_async<'a, Fut>(&'static self, t: &'a T, fut: Fut) -> SetAsync<'a, T, Fut>
    where
        T: 'static,
        Fut: Future;
}

impl<T> ScopedKeyExt<T> for ScopedKey<T> {
    fn set_async<'a, Fut>(&'static self, t: &'a T, fut: Fut) -> SetAsync<'a, T, Fut>
    where
        T: 'static,
        Fut: Future,
    {
        SetAsync { key: self, t, fut }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct SetAsync<'a, T: 'static, Fut> {
    key: &'static ScopedKey<T>,
    t: &'a T,
    fut: Fut,
}

impl<T, Fut> Future for SetAsync<'_, T, Fut>
where
    Fut: Future,
{
    type Output = Fut::Output;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        unsafe {
            let me = self.get_unchecked_mut();
            let key = me.key;
            let t = me.t;
            let fut = Pin::new_unchecked(&mut me.fut);
            key.set(t, || fut.poll(cx))
        }
    }
}

// copied from scoped-tls
#[cfg(test)]
mod tests {
    use crate::{scoped_thread_local, ScopedKeyExt as _};
    use futures::{
        channel::oneshot::{channel, Sender},
        executor::{block_on, LocalPool},
        future::FutureExt as _,
        task::LocalSpawnExt as _,
    };
    use std::{cell::Cell, panic::AssertUnwindSafe};

    scoped_thread_local!(static FOO: u32);

    #[test]
    fn smoke() {
        scoped_thread_local!(static BAR: u32);

        block_on(async {
            assert!(!BAR.is_set());
            BAR.set_async(&1, async {
                assert!(BAR.is_set());
                BAR.with(|slot| {
                    assert_eq!(*slot, 1);
                });
            })
            .await;
            assert!(!BAR.is_set());
        });
    }

    #[test]
    fn cell_allowed() {
        scoped_thread_local!(static BAR: Cell<u32>);

        block_on(async {
            BAR.set_async(&Cell::new(1), async {
                BAR.with(|slot| {
                    assert_eq!(slot.get(), 1);
                });
            })
            .await;
        });
    }

    #[test]
    fn scope_item_allowed() {
        block_on(async {
            assert!(!FOO.is_set());
            FOO.set_async(&1, async {
                assert!(FOO.is_set());
                FOO.with(|slot| {
                    assert_eq!(*slot, 1);
                });
            })
            .await;
            assert!(!FOO.is_set());
        });
    }

    #[test]
    fn panic_resets() {
        struct Check(Option<Sender<u32>>);
        impl Drop for Check {
            fn drop(&mut self) {
                FOO.with(|r| {
                    self.0.take().unwrap().send(*r).unwrap();
                })
            }
        }

        let mut pool = LocalPool::new();
        let spawner = pool.spawner();

        pool.run_until(async {
            let (tx, rx) = channel();
            let t = spawner
                .spawn_local_with_handle(
                    AssertUnwindSafe(async {
                        FOO.set_async(&1, async {
                            let _r = Check(Some(tx));

                            FOO.set_async(&2, async { panic!() }).await;
                        })
                        .await;
                    })
                    .catch_unwind(),
                )
                .unwrap();

            assert_eq!(rx.await.unwrap(), 1);
            assert!(t.await.is_err());
        });
    }
}

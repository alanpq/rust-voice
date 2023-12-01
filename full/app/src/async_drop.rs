use std::{
  mem::MaybeUninit,
  ops::{Deref, DerefMut},
};

#[async_trait::async_trait]
pub trait AsyncDrop {
  async fn async_drop(&mut self);
}

/// Wrapper to provide AsyncDrop implementation
pub struct Dropper<T: AsyncDrop + Send + 'static> {
  dropped: bool,
  inner: MaybeUninit<T>,
}

impl<T: AsyncDrop + Send + 'static> Dropper<T> {
  pub fn new(inner: T) -> Self {
    Self {
      dropped: false,
      inner: MaybeUninit::new(inner),
    }
  }
}

impl<T: AsyncDrop + Send + 'static> Deref for Dropper<T> {
  type Target = T;

  fn deref(&self) -> &Self::Target {
    // Safety: the only uninit exists in the Drop impl for Dropper, so as long as !this.dropped, we can assume_init
    unsafe { self.inner.assume_init_ref() }
  }
}

impl<T: AsyncDrop + Send + 'static> DerefMut for Dropper<T> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { self.inner.assume_init_mut() }
  }
}

impl<T: AsyncDrop + Send + 'static> From<T> for Dropper<T> {
  fn from(value: T) -> Self {
    Self::new(value)
  }
}

impl<T: AsyncDrop + Send + 'static> Drop for Dropper<T> {
  fn drop(&mut self) {
    if !self.dropped {
      let mut this = Dropper {
        dropped: true,
        inner: MaybeUninit::uninit(),
      };
      std::mem::swap(&mut this, self);
      this.dropped = true;

      // TODO: figure out executor confusion (iced, async_std, etc)
      async_std::task::spawn(async move {
        // Safety: the only uninit exists in the Drop impl for Dropper, so as long as !this.dropped, we can assume_init
        let inner = unsafe { this.inner.assume_init_mut() };
        inner.async_drop().await;
      });
    }
  }
}

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use serde::Serialize;

static NEXT_CHANNEL_ID: AtomicU32 = AtomicU32::new(1_000_000);

pub struct StreamChannel<T: Serialize + Send + 'static> {
    id: u32,
    inner: StreamChannelInner<T>,
}

enum StreamChannelInner<T: Serialize + Send + 'static> {
    #[cfg(feature = "desktop")]
    Tauri(tauri::ipc::Channel<T>),
    Callback(Arc<dyn Fn(T) -> Result<(), String> + Send + Sync>),
}

impl<T: Serialize + Send + 'static> Clone for StreamChannel<T> {
    fn clone(&self) -> Self {
        match &self.inner {
            #[cfg(feature = "desktop")]
            StreamChannelInner::Tauri(channel) => Self {
                id: self.id,
                inner: StreamChannelInner::Tauri(channel.clone()),
            },
            StreamChannelInner::Callback(callback) => Self {
                id: self.id,
                inner: StreamChannelInner::Callback(Arc::clone(callback)),
            },
        }
    }
}

impl<T: Serialize + Send + 'static> StreamChannel<T> {
    #[cfg(feature = "desktop")]
    pub fn from_tauri(channel: tauri::ipc::Channel<T>) -> Self {
        let id = channel.id();
        Self {
            id,
            inner: StreamChannelInner::Tauri(channel),
        }
    }

    pub fn from_callback<F>(id: u32, callback: F) -> Self
    where
        F: Fn(T) -> Result<(), String> + Send + Sync + 'static,
    {
        Self {
            id,
            inner: StreamChannelInner::Callback(Arc::new(callback)),
        }
    }

    pub fn allocate_id() -> u32 {
        NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed)
    }

    pub fn from_new_callback<F>(callback: F) -> Self
    where
        F: Fn(T) -> Result<(), String> + Send + Sync + 'static,
    {
        Self::from_callback(Self::allocate_id(), callback)
    }

    #[cfg(test)]
    pub fn from_test_callback<F>(callback: F) -> Self
    where
        F: Fn(T) -> Result<(), String> + Send + Sync + 'static,
    {
        Self::from_new_callback(callback)
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn send(&self, payload: T) -> Result<(), String> {
        match &self.inner {
            #[cfg(feature = "desktop")]
            StreamChannelInner::Tauri(channel) => {
                channel.send(payload).map_err(|error| error.to_string())
            }
            StreamChannelInner::Callback(callback) => callback(payload),
        }
    }
}

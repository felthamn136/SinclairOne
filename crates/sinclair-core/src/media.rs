/// Opaque file bytes and a machine-specific format identifier, not a path.
pub struct Media<'a> {
    pub format: &'a str,
    pub bytes: &'a [u8],
}

/// Codecs belong to the machine crate, never to the shared file browser.
pub trait MediaCodec {
    type Output;

    fn decode(&self, bytes: &[u8]) -> crate::Result<Self::Output>;
}

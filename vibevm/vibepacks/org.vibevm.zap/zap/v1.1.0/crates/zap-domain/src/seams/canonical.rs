macro_rules! impl_canonical {
    ($type:ty) => {
        impl zap_wire::CanonicalEncode for $type {
            fn encode_canonical(
                &self,
                codec: zap_wire::CodecEpoch,
            ) -> Result<zap_wire::CanonicalOutput, zap_wire::ZapError> {
                zap_wire::CanonicalOutput::encode_json(codec, self)
            }
        }

        impl zap_wire::CanonicalDecode for $type {
            fn decode_canonical(
                payload: &zap_wire::CanonicalPayload,
            ) -> Result<Self, zap_wire::ZapError> {
                payload.decode_json()
            }
        }
    };
}

pub(crate) use impl_canonical;

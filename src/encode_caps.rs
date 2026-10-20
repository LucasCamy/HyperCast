//! Driver-advertised encode constraints, never a decode/performance guarantee.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct EncodeCaps {
    pub support: u32,
    pub min_width: u32,
    pub min_height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub dynamic_bitrate: u32,
    pub align_height: u32,
    pub max_fps: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Codec,
    Dimensions,
    Fps,
    DynamicBitrate,
}
impl Rejection {
    pub fn label(self) -> &'static str {
        match self {
            Self::Codec => crate::i18n::label(
                "Este encoder não oferece o codec selecionado. Escolha outro formato.",
                "This encoder does not offer the selected codec. Choose another format.",
            ),
            Self::Dimensions => crate::i18n::label(
                "Resolução fora dos limites informados pelo encoder. Escolha outra resolução de envio.",
                "Resolution exceeds the encoder's advertised limits. Choose another output resolution.",
            ),
            Self::DynamicBitrate => crate::i18n::label(
                "Este encoder não anuncia reconfiguração de bitrate. Use Manual ou outro encoder.",
                "This encoder does not advertise live bitrate changes. Use Manual or another encoder.",
            ),
            Self::Fps => crate::i18n::label(
                "FPS acima do limite informado pelo encoder. Reduza o FPS de envio.",
                "FPS exceeds the encoder's advertised limit. Lower the sending FPS.",
            ),
        }
    }
}
impl EncodeCaps {
    pub fn automatic_rejection(self) -> Option<Rejection> {
        (self.dynamic_bitrate == 2).then_some(Rejection::DynamicBitrate)
    }
    pub fn rejection(self, w: u32, h: u32, fps: u32) -> Option<Rejection> {
        if self.support == 2 {
            return Some(Rejection::Codec);
        }
        // Invalid or contradictory driver data is unknown, never a reason to disable UI.
        let bound = |min, max, v| {
            let low = (1..=32768).contains(&min);
            let high = (1..=32768).contains(&max);
            if low && high && min > max {
                return false;
            }
            (low && v < min) || (high && v > max)
        };
        if bound(self.min_width, self.max_width, w) || bound(self.min_height, self.max_height, h) {
            return Some(Rejection::Dimensions);
        }
        if (1..=1000).contains(&self.max_fps) && fps > self.max_fps {
            return Some(Rejection::Fps);
        }
        None
    }
    pub fn hint(self) -> &'static str {
        if self.support == 1 {
            crate::i18n::label(
                "Suporte de encode informado pelo driver; desempenho depende da sessão.",
                "Driver advertises encode support; performance depends on the session.",
            )
        } else {
            crate::i18n::label(
                "Capacidade desconhecida; você pode tentar iniciar o envio.",
                "Capability unknown; you can try starting the stream.",
            )
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_never_implies_incompatible_or_decode_support() {
        assert_eq!(EncodeCaps::default().rejection(4588, 1920, 120), None);
        assert_eq!(std::mem::size_of::<EncodeCaps>(), 32);
        assert_eq!(EncodeCaps::default().automatic_rejection(), None);
        assert_eq!(
            EncodeCaps {
                dynamic_bitrate: 2,
                ..Default::default()
            }
            .automatic_rejection(),
            Some(Rejection::DynamicBitrate)
        );
    }
    #[test]
    fn constraints_are_per_codec_and_requested_output() {
        let caps = EncodeCaps {
            support: 1,
            max_width: 4096,
            max_height: 4096,
            max_fps: 60,
            ..Default::default()
        };
        assert_eq!(caps.rejection(4588, 1920, 60), Some(Rejection::Dimensions));
        assert_eq!(caps.rejection(2560, 1440, 120), Some(Rejection::Fps));
        assert_eq!(caps.rejection(2560, 1440, 60), None);
        assert_eq!(
            EncodeCaps {
                support: 2,
                ..Default::default()
            }
            .rejection(1920, 1080, 30),
            Some(Rejection::Codec)
        );
    }
    #[test]
    fn corrupt_limits_and_missing_fields_do_not_disable_valid_choices() {
        let caps = EncodeCaps {
            support: 99,
            min_width: 9000,
            max_width: 4096,
            max_height: u32::MAX,
            max_fps: u32::MAX,
            ..Default::default()
        };
        assert_eq!(caps.rejection(4588, 1920, 120), None);
        assert_eq!(
            EncodeCaps {
                min_width: 16,
                max_width: u32::MAX,
                ..Default::default()
            }
            .rejection(8, 480, 30),
            Some(Rejection::Dimensions)
        );
    }
}

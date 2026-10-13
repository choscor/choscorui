//! ImageIO types and functions.

use super::cf::{CFData, CFDictionary, CFMutableData, CFString, CfRetained, cf_type};
use super::graphics::CGImage;
use super::sdk::c_functions;

cf_type!(CGImageSource, CGImageDestination);

c_functions! {
    #[link(name = "ImageIO", kind = "framework")]
    functions {
        pub(crate) safe fn CGImageSourceCreateWithData(
            data: &CFData,
            options: Option<&CFDictionary>,
        ) -> Option<CfRetained<CGImageSource>>;
        pub(crate) safe fn CGImageSourceGetCount(source: &CGImageSource) -> usize;
        pub(crate) safe fn CGImageSourceCreateImageAtIndex(
            source: &CGImageSource,
            index: usize,
            options: Option<&CFDictionary>,
        ) -> Option<CfRetained<CGImage>>;
        pub(crate) safe fn CGImageDestinationCreateWithData(
            data: &CFMutableData,
            kind: &CFString,
            count: usize,
            options: Option<&CFDictionary>,
        ) -> Option<CfRetained<CGImageDestination>>;
        pub(crate) safe fn CGImageDestinationAddImage(
            destination: &CGImageDestination,
            image: &CGImage,
            properties: Option<&CFDictionary>,
        );
        pub(crate) safe fn CGImageDestinationFinalize(destination: &CGImageDestination) -> bool;
    }
}

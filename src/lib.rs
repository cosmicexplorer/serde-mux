/*
 * Description: Select among serde formats.
 *
 * Copyright (C) 2022 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: LGPL-3.0-or-later
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Lesser General Public License as published
 * by the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Select among serde formats.

/* #![warn(missing_docs)] */
#![deny(rustdoc::missing_crate_level_docs)]
/* Make all doctests fail if they produce any warnings. */
#![doc(test(attr(deny(warnings))))]
#![deny(clippy::all)]

//! Serialization and deserialization (ser/de) mechanisms for this crate's objects.

pub use traits::*;
pub mod traits {
  pub trait Schema {
    type Source;
  }

  pub trait SerializationFormat {
    type Read: ?Sized;
    type Written: Sized;
  }

  pub trait SerdeViaBase {
    type Fmt: SerializationFormat;
    type Medium: Schema;
  }

  pub trait Serializer: SerdeViaBase {
    fn serialize(self) -> <Self::Fmt as SerializationFormat>::Written;
  }

  pub trait Deserializer: SerdeViaBase {
    type Error;
    fn deserialize(
      data: &<Self::Fmt as SerializationFormat>::Read,
    ) -> Result<<Self::Medium as Schema>::Source, Self::Error>;
  }

  pub trait SerdeVia: Serializer+Deserializer {}
}

pub mod fingerprinting {
  use super::traits::Schema;

  use hex;

  use std::{convert::AsRef, marker::PhantomData};

  #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
  pub struct FingerprintableBytes<Source>(Box<[u8]>, PhantomData<Source>);

  #[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
  pub struct HexFingerprint<Source>(String, PhantomData<Source>);

  impl<Source> From<String> for HexFingerprint<Source> {
    fn from(value: String) -> Self { Self(value, PhantomData) }
  }
  impl<Source> From<HexFingerprint<Source>> for String {
    fn from(value: HexFingerprint<Source>) -> Self { value.0 }
  }
  impl<Source> AsRef<str> for HexFingerprint<Source> {
    fn as_ref(&self) -> &str { self.0.as_ref() }
  }

  impl<Source> FingerprintableBytes<Source> {
    pub fn new(bytes: Box<[u8]>) -> Self { Self(bytes, PhantomData) }

    pub fn from_hex_string(hex_string: &str) -> Result<Self, hex::FromHexError> {
      let decoded: Vec<u8> = hex::decode(hex_string)?;
      Ok(Self::new(decoded.into_boxed_slice()))
    }

    pub fn into_hex_string(self) -> HexFingerprint<Source> {
      HexFingerprint::from(hex::encode(&self.0))
    }
  }

  impl<Source> Schema for FingerprintableBytes<Source> {
    type Source = Source;
  }

  pub trait Fingerprintable: Into<FingerprintableBytes<Self>> {}
}

pub use formats::key_fingerprint::KeyFingerprint;
#[cfg(feature = "protobuf")]
pub use formats::protobuf::{Protobuf, ProtobufCodingFailure};
pub mod formats {
  use super::traits::*;

  use std::marker::PhantomData;

  pub mod key_fingerprint {
    use super::{super::fingerprinting::*, *};

    #[derive(Debug, Copy, Clone)]
    pub struct KeyFingerprintFormat<Source>(PhantomData<Source>);

    impl<Source> SerializationFormat for KeyFingerprintFormat<Source> {
      type Read = str;
      type Written = HexFingerprint<Source>;
    }

    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct KeyFingerprint<Source>(Source);

    impl<Source> KeyFingerprint<Source> {
      pub fn new(source: Source) -> Self { Self(source) }
    }

    impl<Source> SerdeViaBase for KeyFingerprint<Source>
    where Source: Fingerprintable
    {
      type Fmt = KeyFingerprintFormat<Source>;
      type Medium = FingerprintableBytes<Source>;
    }

    impl<Source> Serializer for KeyFingerprint<Source>
    where Source: Fingerprintable
    {
      fn serialize(self) -> HexFingerprint<Source> {
        let proto_message: FingerprintableBytes<_> = self.0.into();
        proto_message.into_hex_string()
      }
    }
  }

  #[cfg(feature = "protobuf")]
  pub mod protobuf {
    use super::*;

    use displaydoc::Display;
    use thiserror::Error;

    use std::{convert::TryInto, default::Default};

    #[derive(Debug, Copy, Clone)]
    pub struct ProtobufFormat;

    impl SerializationFormat for ProtobufFormat {
      type Read = [u8];
      type Written = Box<[u8]>;
    }

    #[derive(Debug, Copy, Clone)]
    pub struct Protobuf<Source, Proto>(pub Source, PhantomData<Proto>);

    impl<Source, Proto> Protobuf<Source, Proto> {
      pub fn new(source: Source) -> Self { Self(source, PhantomData) }
    }

    impl<Proto> SerdeViaBase for Protobuf<Proto::Source, Proto>
    where Proto: Schema
    {
      type Fmt = ProtobufFormat;
      type Medium = Proto;
    }

    impl<Proto> Serializer for Protobuf<Proto::Source, Proto>
    where Proto: Schema+prost::Message+From<Proto::Source>
    {
      fn serialize(self) -> Box<[u8]> {
        let proto_message: Proto = self.0.into();
        proto_message.encode_to_vec().into_boxed_slice()
      }
    }

    impl<Proto, E> Deserializer for Protobuf<Proto::Source, Proto>
    where
      E: From<prost::DecodeError>,
      Proto: Schema+prost::Message+TryInto<Proto::Source, Error=E>+Default,
    {
      type Error = E;

      fn deserialize(data: &[u8]) -> Result<Proto::Source, Self::Error> {
        let proto_message = Proto::decode(data)?;
        proto_message.try_into()
      }
    }

    impl<Proto, E> SerdeVia for Protobuf<Proto::Source, Proto>
    where
      E: From<prost::DecodeError>,
      Proto: Schema+prost::Message+From<Proto::Source>+TryInto<Proto::Source, Error=E>+Default,
    {
    }

    /// Error type for specifics on failures to serialize or deserialize a protobuf-backed object.
    #[derive(Debug, Display, Error)]
    pub enum ProtobufCodingFailure {
      /// an optional field '{0}' was absent when en/decoding protobuf {1}
      OptionalFieldAbsent(&'static str, String),
      /// an invalid state {0} was detected when en/decoding protobuf {1}
      FieldCompositionWasIncorrect(String, String),
      /// an error {1} occured trying to coax a byte slice to the correct length {0}
      SliceLength(usize, String),
      /// an error {0} occurred when en/decoding a protobuf map for the type {1}
      MapStringCodingFailed(String, String),
      /// a prost encoding error {0} was raised internally
      Encode(#[from] prost::EncodeError),
      /// a prost decoding error {0} was raised internally
      Decode(#[from] prost::DecodeError),
    }
  }
}

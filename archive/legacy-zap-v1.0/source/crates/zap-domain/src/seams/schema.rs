macro_rules! schema_tag {
    ($name:ident, $wire:literal, $documents:literal) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
        #[specmark::spec(documents = $documents)]
        pub enum $name {
            #[serde(rename = $wire)]
            V1,
        }
    };
}

pub(crate) use schema_tag;

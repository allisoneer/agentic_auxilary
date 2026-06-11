use super::location::LocationInfo;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DataEnvelope<T> {
    pub data: T,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CursorLinks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CursorEnvelope<T> {
    pub data: T,
    pub cursor: CursorLinks,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocationEnvelope<T> {
    pub location: LocationInfo,
    pub data: T,
}

impl<'de, T> Deserialize<'de> for DataEnvelope<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<T> {
            Envelope { data: T },
            Bare(T),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Envelope { data } | Wire::Bare(data) => Ok(Self { data }),
        }
    }
}

impl<'de, T> Deserialize<'de> for CursorEnvelope<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<T> {
            Envelope {
                data: T,
                #[serde(default)]
                cursor: CursorLinks,
            },
            Bare(T),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Envelope { data, cursor } => Ok(Self { data, cursor }),
            Wire::Bare(data) => Ok(Self {
                data,
                cursor: CursorLinks::default(),
            }),
        }
    }
}

impl<'de, T> Deserialize<'de> for LocationEnvelope<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<T> {
            Envelope {
                #[serde(default)]
                location: LocationInfo,
                data: T,
            },
            Bare(T),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Envelope { location, data } => Ok(Self { location, data }),
            Wire::Bare(data) => Ok(Self {
                location: LocationInfo::default(),
                data,
            }),
        }
    }
}

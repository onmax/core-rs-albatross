use nimiq_handel::{contribution::AggregatableContribution, update::LevelUpdate};
use nimiq_primitives::policy::Policy;
use serde::{
    de::{Error as _, Unexpected},
    Deserialize, Deserializer, Serialize, Serializer,
};

/// The serializable/deserializable representation of a LevelUpdate. It does omit the origin of the
/// LevelUpdate itself, as the ValidatorNetwork's ValidatorMessage already includes it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "C: AggregatableContribution")]
pub struct SerializableLevelUpdate<C>
where
    C: AggregatableContribution,
{
    aggregate: Checked<C>,
    individual: Option<Checked<C>>,
    level: u8,
}

#[derive(Clone, Debug)]
struct Checked<C: AggregatableContribution>(C);

impl<'de, C: AggregatableContribution> Deserialize<'de> for Checked<C> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let result = C::deserialize(deserializer)?;
        for slot in result.contributors().iter() {
            if slot >= Policy::SLOTS as usize {
                return Err(D::Error::invalid_value(
                    Unexpected::Unsigned(slot.try_into().unwrap()),
                    &"slot must be smaller than 512",
                ));
            }
        }
        Ok(Checked(result))
    }
}

impl<C: AggregatableContribution> Serialize for Checked<C> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Serialize::serialize(&self.0, serializer)
    }
}

impl<C> SerializableLevelUpdate<C>
where
    C: AggregatableContribution,
{
    /// Given an origin, transforms this SerializableLevelUpdate into a LevelUpdate.
    pub fn into_level_update(self, origin: u16) -> LevelUpdate<C> {
        LevelUpdate {
            aggregate: self.aggregate.0,
            individual: self.individual.map(|contribution| contribution.0),
            level: self.level,
            origin,
        }
    }
}

impl<C> From<LevelUpdate<C>> for SerializableLevelUpdate<C>
where
    C: AggregatableContribution,
{
    fn from(value: LevelUpdate<C>) -> Self {
        Self {
            aggregate: Checked(value.aggregate),
            individual: value.individual.map(Checked),
            level: value.level,
        }
    }
}

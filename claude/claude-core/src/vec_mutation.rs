//! Vector mutation utilities

/// Mutations that can be applied to a vector
#[derive(Debug, Clone, PartialEq)]
pub enum VecMutation<T: Clone> {
    /// Push a new element
    Push(T),
    /// Update the last element
    UpdateLast(T),
    /// Replace the entire vector
    Set(Vec<T>),
    /// Clear all elements
    Clear,
    /// Remove element at index
    RemoveAt(usize),
}

impl<T: Clone> VecMutation<T> {
    /// Apply this mutation to the given vector
    pub fn apply(self, list: &mut Vec<T>) {
        match self {
            Self::Push(v) => list.push(v),
            Self::UpdateLast(v) => {
                if let Some(last) = list.last_mut() {
                    *last = v;
                }
            }
            Self::Set(vs) => *list = vs,
            Self::Clear => list.clear(),
            Self::RemoveAt(idx) => {
                if idx < list.len() {
                    list.remove(idx);
                }
            }
        }
    }
}

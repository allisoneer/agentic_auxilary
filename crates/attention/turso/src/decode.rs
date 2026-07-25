use crate::Error;
use turso_db::Row;

pub fn text(row: &Row, index: usize) -> Result<String, Error> {
    row.get(index)
        .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn integer(row: &Row, index: usize) -> Result<i64, Error> {
    row.get(index)
        .map_err(|error| Error::Decode(Box::new(error)))
}

pub fn blob(row: &Row, index: usize) -> Result<Vec<u8>, Error> {
    row.get(index)
        .map_err(|error| Error::Decode(Box::new(error)))
}

use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
static MESSAGE_ID_STATE: Mutex<(u64, u16)> = Mutex::new((0, 0));

pub fn new_message_id() -> String {
    format!("msg_{}{}", timestamp_hex_12(), random_base62(14))
}

fn timestamp_hex_12() -> String {
    let current_timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis() as u64,
        Err(error) => panic!("system time should be after unix epoch: {error}"),
    };

    let mut state = match MESSAGE_ID_STATE.lock() {
        Ok(state) => state,
        Err(error) => panic!("message id state poisoned: {error}"),
    };
    if state.0 != current_timestamp {
        state.0 = current_timestamp;
        state.1 = 0;
    }
    state.1 = state.1.wrapping_add(1);

    let encoded = current_timestamp
        .saturating_mul(0x1000)
        .saturating_add(u64::from(state.1));

    format!("{:012x}", encoded & 0xffff_ffff_ffff)
}

fn random_base62(length: usize) -> String {
    let mut result = String::with_capacity(length);

    while result.len() < length {
        for byte in uuid::Uuid::new_v4().into_bytes() {
            result.push(BASE62[(byte % 62) as usize] as char);

            if result.len() == length {
                break;
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_message_id_shape() {
        let id = new_message_id();
        let suffix = id
            .strip_prefix("msg_")
            .expect("message IDs start with msg_");
        let (timestamp, random) = suffix.split_at(12);

        assert_eq!(id.len(), 30);
        assert!(timestamp.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_eq!(random.len(), 14);
        assert!(random.chars().all(|ch| ch.is_ascii_alphanumeric()));
        assert!(random.chars().all(|ch| BASE62.contains(&(ch as u8))));
    }

    #[test]
    fn test_new_message_id_is_unique_across_two_calls() {
        assert_ne!(new_message_id(), new_message_id());
    }
}

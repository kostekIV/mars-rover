use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::xdr::{LedgerEntry, LedgerKey, Limits, WriteXdr};
use soroban_env_host::HostError;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Default, Clone)]
pub struct Memory {
    memory: RefCell<BTreeMap<Rc<LedgerKey>, (Rc<LedgerEntry>, Option<u32>)>>,
}
use std::fmt;

impl fmt::Debug for Memory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let memory = self.memory.borrow();

        let mut map = f.debug_map();
        for (key, (entry, ttl)) in memory.iter() {
            // Format key and entry more concisely
            let key_str = format!("{:?}", key);
            let entry_str = format!("Entry({:?}), TTL: {:?}", entry.data, ttl);
            map.entry(&key_str, &entry_str);
        }
        map.finish()
    }
}

impl Memory {
    pub fn insert(&self, entry: LedgerEntry) {
        self.insert_with_ttl(entry, None);
    }

    pub fn insert_with_ttl(&self, entry: LedgerEntry, ttl: Option<u32>) {
        self.memory
            .borrow_mut()
            .insert(Rc::new(entry.to_key()), (Rc::new(entry), ttl));
    }

    pub fn update_ttl(&self, key: &Rc<LedgerKey>, new_ttl: Option<u32>) {
        self.memory
            .borrow_mut()
            .entry(key.clone())
            .and_modify(|(_, ttl)| *ttl = new_ttl);
    }

    pub fn remove(&self, key: &Rc<LedgerKey>) {
        let entry = self.memory.borrow_mut().remove(key);
    }
}

impl SnapshotSource for Memory {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
       // println!("get {:?}", key);
        let entry = self.memory.borrow().get(key).cloned();
        // println!(
        //     "return {:?}",
        //     entry
        //         .as_ref()
        //         .map(|(x, y)| x.data.to_xdr_base64(Limits::none()))
        // );
        Ok(entry)
    }
}

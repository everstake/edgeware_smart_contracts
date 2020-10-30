#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract]
mod edgeware_bridge {
    use erc20token::ERC20Token;

    use ink_prelude::vec::Vec;
    use ink_prelude::string::String;

    use ink_storage::{collections::{HashMap as StorageHashMap},
                    traits::{
                        PackedLayout,
                        SpreadLayout,
                    },
                    Lazy};
    use scale::{Decode, Encode};
    use ink_env::hash::Keccak256;

    use ed25519_compact::*;

    const ZERO_ADDRESS_BYTES: [u8; 32] = [0; 32];

    #[ink(storage)]
    pub struct EdgewareBridge {
        token_balances: StorageHashMap<AccountId, Balance>,
        tokens: StorageHashMap<AccountId, bool>,
        validators: StorageHashMap<Vec<u8>, bool>, // raw PublicKey as a key
        workers: StorageHashMap<AccountId, bool>,
        daily_limit: StorageHashMap<AccountId, u128>,
        daily_spend: StorageHashMap<AccountId, u128>,
        daily_limit_set_time: StorageHashMap<AccountId, u64>,
        fee: u128,
        signature_threshold: u16,
        max_validator_count: u16,
        tx_expiration_time: u64,
        owner: AccountId,
        transfer_nonce: u128,
        token_contract: Lazy<ERC20Token>,
    }

    /// Emitted when an user want to make cross chain transfer
    #[ink(event)]
    pub struct Transfer {
        receiver: String,
        sender: AccountId,
        amount: u128,
        asset: AccountId,
        transfer_nonce: u128,
        timestamp: u64,
    }

    #[derive(Encode, Decode, SpreadLayout, PackedLayout, Clone)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink_storage::traits::StorageLayout
        )
    )]
    pub struct SwapMessage {
        pub chain_id: u8,
        pub receiver: [u8; 32],
        pub sender: String,
        pub timestamp: u64,
        pub amount: u128,
        pub asset: [u8; 32],
        pub transfer_nonce: u128,
    }

    #[derive(Encode, Decode, SpreadLayout, PackedLayout)]
    #[cfg_attr(
        feature = "std",
        derive(
            Debug,
            PartialEq,
            Eq,
            scale_info::TypeInfo,
            ink_storage::traits::StorageLayout
        )
    )]
    pub struct EdSignature {
        pub signature_data: Vec<u8>,
        pub signer: Vec<u8>,
    }

    impl EdgewareBridge {
        #[ink(constructor)]
        pub fn new(
            threshold: u16,
            max_permissible_validator_count: u16,
            transfer_fee: u128,
            coin_daily_limit: u128,
            token_code_hash: Hash,
        ) -> Self {
            let total_balance = Self::env().balance();
            let caller = Self::env().caller();
            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            let mut daily_limit: StorageHashMap<AccountId, u128> = StorageHashMap::default();
            let current_timestamp: u64 = Self::env().block_timestamp();
            daily_limit.insert(zero_address, coin_daily_limit);
            let mut daily_limit_set_time: StorageHashMap<AccountId, u64> = StorageHashMap::default();
            daily_limit_set_time.insert(zero_address, current_timestamp);
            let token = ERC20Token::new(String::from("PolkaDot"), String::from("DOT"))
                .endowment(total_balance / 2)
                .code_hash(token_code_hash)
                .instantiate()
                .expect("failed at instantiating the `Token` contract");
            Self {
                token_balances: StorageHashMap::default(),
                tokens: StorageHashMap::default(),
                validators: StorageHashMap::default(),
                workers: StorageHashMap::default(),
                daily_spend: StorageHashMap::default(),
                fee: transfer_fee,
                signature_threshold: threshold,
                max_validator_count: max_permissible_validator_count,
                tx_expiration_time: 86400,
                owner: caller,
                transfer_nonce: 0,
                daily_limit,
                daily_limit_set_time,
                token_contract: Lazy::new(token),
            }
        }

        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) {
            self.ensure_owner(self.env().caller());
            self.owner = new_owner;
        }

        #[ink(message)]
        pub fn set_fee(&mut self, new_fee: u128) {
            self.ensure_owner(self.env().caller());
            self.fee = new_fee;
        }

        #[ink(message)]
        pub fn add_validator(&mut self, new_validator: Vec<u8>) {
            self.ensure_owner(self.env().caller());
            self.validators.insert(new_validator, true);
        }

        #[ink(message)]
        pub fn remove_validator(&mut self, validator_pub_key: Vec<u8>) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.validators.take(&validator_pub_key).is_some(), true);
        }

        #[ink(message)]
        pub fn add_worker(&mut self, new_worker: AccountId) {
            self.ensure_owner(self.env().caller());
            self.workers.insert(new_worker, true);
        }

        #[ink(message)]
        pub fn remove_worker(&mut self, worker: AccountId) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.workers.take(&worker).is_some(), true);
        }

        #[ink(message)]
        pub fn set_threshold(&mut self, new_signature_threshold: u16) {
            self.ensure_owner(self.env().caller());
            self.signature_threshold = new_signature_threshold;
        }

        #[ink(message)]
        pub fn add_token(&mut self, new_token: AccountId, token_daily_limit: u128) {
            self.ensure_owner(self.env().caller());
            self.tokens.insert(new_token, true);
            self.daily_limit.insert(new_token, token_daily_limit);
            self.daily_limit_set_time.insert(new_token, self.env().block_timestamp());
        }

        #[ink(message)]
        pub fn remove_token(&mut self, token: AccountId) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.tokens.take(&token).is_some(), true);
            assert_eq!(self.daily_limit.take(&token).is_some(), true);
            assert_eq!(self.daily_limit_set_time.take(&token).is_some(), true);
        }

        #[ink(message)]
        pub fn set_daily_limit(&mut self, new_limit: u128, asset_limited: AccountId) {
            self.ensure_owner(self.env().caller());
            assert_eq!(self.tokens.contains_key(&asset_limited), true);
            self.daily_limit.insert(asset_limited, new_limit);
        }

        #[ink(message)]
        pub fn set_tx_expiration_time(&mut self, new_tx_expiration_time: u64) {
            self.ensure_owner(self.env().caller());
            self.tx_expiration_time = new_tx_expiration_time;
        }

        // DEV method, remove in future
        #[ink(message)]
        pub fn get_signer(&self, transfer_info: SwapMessage, ed_sig: EdSignature) -> bool {
            let signature: Signature = Signature::from_slice(ed_sig.signature_data.as_slice()).unwrap();
            let message_hash: [u8;32] = self.hash_message(transfer_info);
            let pub_k: PublicKey = PublicKey::from_slice(ed_sig.signer.as_slice()).unwrap();
            
            pub_k.verify(message_hash.as_ref(), &signature).is_ok()
        }

        // DEV method, remove in future
        #[ink(message)]
        pub fn get_hash(&self, transfer_info: SwapMessage) -> [u8;32] {
            let message_hash: [u8;32] = self.hash_message(transfer_info);
            message_hash
        }

        #[ink(message)]
        pub fn request_swap(&mut self, transfer_info: SwapMessage, signatures: Vec<EdSignature>) -> bool {
            assert!(self.check_expiration_time(transfer_info.timestamp.clone()), "Transaction can't be sent because of expiration time");

            let asset: AccountId = AccountId::from(transfer_info.asset);

            assert!(self.check_asset(&asset), "Unknown asset is trying to transfer");

            let receiver_address: AccountId = AccountId::from(transfer_info.receiver);

            assert!(signatures.len() as u16 >= self.signature_threshold && signatures.len() as u16 <= self.max_validator_count, "Wrong count of signatures to make transfer");

            let message_hash: [u8;32] = self.hash_message(transfer_info.clone());

            assert!(self.verify_signatures(message_hash.as_ref(), &signatures),  "Signatures verification is failed");

            let res: bool = self.make_swap(asset, transfer_info.amount.clone(), receiver_address);

            res
        }

        #[ink(message)]
        pub fn transfer_coin(&mut self, receiver: String) -> bool {
            let attached_deposit: u128 = self.env().transferred_balance();
            
            assert!(attached_deposit > 0, "You have to attach some amount of assets to make transfer");

            self.increase_transfer_nonce();

            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);

            self.env().emit_event(Transfer {
                receiver,
                sender: self.env().caller(),
                amount: attached_deposit,
                asset: zero_address,
                transfer_nonce: self.transfer_nonce,
                timestamp: self.env().block_timestamp(),
            });
            true
        }

        #[ink(message)]
        pub fn transfer_token(&mut self, receiver: String, amount: u128, asset: AccountId) -> bool {
            assert!(self.check_asset(&asset), "Unknown asset is trying to transfer");
            let caller: AccountId = self.env().caller();
            assert!(self.token_contract.balance_of(caller) >= amount, "Sender doesn't have enough tokens to make transfer");
            assert!(self.token_contract.burn(amount.clone(), caller), "Error while burn sender's tokens");
            self.increase_transfer_nonce();

            self.env().emit_event(Transfer {
                receiver,
                sender: self.env().caller(),
                amount,
                asset,
                transfer_nonce: self.transfer_nonce,
                timestamp: self.env().block_timestamp(),
            });
            true
        }

        fn increase_transfer_nonce(&mut self) {
            self.transfer_nonce = self.transfer_nonce + 1;
        }

        fn check_asset(&self, asset_address: &AccountId) -> bool {
            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            if asset_address == &zero_address || self.tokens.contains_key(asset_address) {
                return true;
            } else {
                return false;
            }
        }

        fn hash_message(&self, swap_message: SwapMessage) -> [u8; 32] {
            let output: [u8;32] = self.env().hash_encoded::<Keccak256, _>(&swap_message);
            output
        }

        fn check_expiration_time(&self, tx_time: u64) -> bool {
            let current_time: u64 = self.env().block_timestamp();
            
            if current_time - tx_time > self.tx_expiration_time {
                return false;
            } else {
                return true;
            }
        }

        fn verify_signatures(&self, signer_message: &[u8], signatures: &Vec<EdSignature>) -> bool {
            let mut signers: Vec<PublicKey> = Vec::new();
            
            for signature in signatures.iter() {
                let pub_k: PublicKey = PublicKey::from_slice(signature.signer.as_slice()).unwrap();
                let signature_struct: Signature = Signature::from_slice(signature.signature_data.as_slice()).unwrap();
                let verified: bool = pub_k.verify(signer_message, &signature_struct).is_ok();
                if !verified || !self.validators.contains_key(&signature.signer) {
                    return false;
                }
                if signers.len() > 0 {
                    if !self.check_unique(&pub_k, &signers) {
                        return false;
                    }
                }
                signers.push(pub_k);
            }
            true
        }

        fn check_unique(&self, new_signer: &PublicKey, all_signers: &Vec<PublicKey>) -> bool {
            for signer in all_signers.iter() {
                if signer == new_signer {
                    return false;
                }
            }
            true
        }

        fn update_daily_limit(&mut self, asset: &AccountId) {
            let current_time: u64 = self.env().block_timestamp();

            let last_limit_set_time: &u64 = self.daily_limit_set_time.get(asset).unwrap();
            if current_time - last_limit_set_time > 86400 {
                self.daily_limit_set_time.insert(asset.clone(), current_time);
                self.daily_spend.insert(asset.clone(), 0);
            }
        }

        fn make_swap(&mut self, asset: AccountId, amount: u128, receiver: AccountId) -> bool {
            let asset_daily_limit: u128 = self.daily_limit.get(&asset).unwrap().clone();

            assert!(asset_daily_limit > 0, "Can't transfer asset without daily limit");

            self.update_daily_limit(&asset);

            let asset_daily_spent: u128 = self.daily_spend.get(&asset).unwrap().clone();

            assert!(amount + asset_daily_spent <= asset_daily_limit);

            self.daily_spend.insert(asset, asset_daily_spent + amount);

            let zero_address: AccountId = AccountId::from(ZERO_ADDRESS_BYTES);
            
            if asset == zero_address {
                let amount_to_send: u128 = amount - (amount * self.fee / 100);
                assert!(self.env().transfer(receiver, amount_to_send).is_ok(), "Error while transfer coins to the receiver");
            } else {
                let amount_to_send: u128 = amount - (amount * self.fee / 100);
                assert!(self.token_contract.mint(amount_to_send, receiver), "Error while mint tokens for the receiver");
            }
            
            true
        }

        fn ensure_owner(&self, caller: AccountId) {
            assert_eq!(caller, self.owner);
        }
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
    }
}

#![cfg_attr(not(feature = "std"), no_std)]

pub use self::erc20token::ERC20Token;
use ink_lang as ink;

#[ink::contract]
pub mod erc20token {
    use ink_storage::collections::HashMap as StorageHashMap;
    use scale::alloc::string::String;

    #[ink(storage)]
    pub struct ERC20Token {
        total_supply: u128,
        balances: StorageHashMap<AccountId, Balance>,
        allowances: StorageHashMap<(AccountId, AccountId), Balance>,
        name: String,
        symbol: String,
        decimals: u8,
        owner: AccountId,
        bridge: Option<AccountId>,
    }

    #[ink(event)]
    pub struct Transfer {
        #[ink(topic)]
        from: Option<AccountId>,
        #[ink(topic)]
        to: Option<AccountId>,
        #[ink(topic)]
        value: Balance,
    }

    #[ink(event)]
    pub struct Approval {
        #[ink(topic)]
        owner: AccountId,
        #[ink(topic)]
        spender: AccountId,
        #[ink(topic)]
        value: Balance,
    }

    #[ink(event)]
    pub struct Mint {
        receiver: AccountId,
        amount: Balance,
    }

    #[ink(event)]
    pub struct Burn {
        holder: AccountId,
        amount: Balance,
    }

    impl ERC20Token {
        /// Creates a new ERC-20 contract with the specified initial supply.
        #[ink(constructor)]
        pub fn new(name: String, symbol: String) -> Self {
            let caller = Self::env().caller();
            let instance = Self {
                total_supply: 0,
                balances: StorageHashMap::new(),
                allowances: StorageHashMap::new(),
                name,
                symbol,
                decimals: 18,
                owner: caller,
                bridge: None,
            };
            instance
        }

        #[ink(message)]
        pub fn transfer_ownership(&mut self, new_owner: AccountId) -> bool {
            self.ensure_owner(self.env().caller());

            self.owner = new_owner;

            true
        }

        #[ink(message)]
        pub fn add_bridge_address(&mut self, bridge_address: AccountId) ->bool {
            self.ensure_owner(self.env().caller());

            self.bridge = Some(bridge_address);

            true
        }

        /// Returns the total token supply.
        #[ink(message)]
        pub fn total_supply(&self) -> u128 {
            self.total_supply
        }

        /// Returns the account balance for the specified `owner`.
        ///
        /// Returns `0` if the account is non-existent.
        #[ink(message)]
        pub fn balance_of(&self, owner: AccountId) -> Balance {
            self.balances.get(&owner).copied().unwrap_or(0)
        }

        /// Returns the amount which `spender` is still allowed to withdraw from `owner`.
        ///
        /// Returns `0` if no allowance has been set `0`.
        #[ink(message)]
        pub fn allowance(&self, owner: AccountId, spender: AccountId) -> Balance {
            self.allowances.get(&(owner, spender)).copied().unwrap_or(0)
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        #[ink(message)]
        pub fn transfer(&mut self, to: AccountId, value: Balance) -> bool {
            let from = self.env().caller();
            self.transfer_from_to(from, to, value)
        }

        /// Allows `spender` to withdraw from the caller's account multiple times, up to
        /// the `value` amount.
        ///
        /// If this function is called again it overwrites the current allowance with `value`.
        ///
        /// An `Approval` event is emitted.
        #[ink(message)]
        pub fn approve(&mut self, spender: AccountId, value: Balance) -> bool {
            let owner = self.env().caller();
            self.allowances.insert((owner, spender), value);
            self.env().emit_event(Approval {
                owner,
                spender,
                value,
            });
            true
        }

        /// Transfers `value` tokens on the behalf of `from` to the account `to`.
        ///
        /// This can be used to allow a contract to transfer tokens on ones behalf and/or
        /// to charge fees in sub-currencies, for example.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientAllowance` error if there are not enough tokens allowed
        /// for the caller to withdraw from `from`.
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the the account balance of `from`.
        #[ink(message)]
        pub fn transfer_from(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> bool {
            let caller = self.env().caller();
            let allowance = self.allowance(from, caller);
            if allowance < value {
                return false;
            }
            if !self.transfer_from_to(from, to, value) {
                return false;
            }
            self.allowances.insert((from, caller), allowance - value);
            true
        }

        #[ink(message)]
        pub fn mint(&mut self, amount: u128, receiver: AccountId) -> bool {
            self.ensure_owner_or_bridge(self.env().caller());

            let receiver_balance = self.balance_of(receiver.clone());
            self.balances.insert(receiver.clone(), receiver_balance + amount);
            self.total_supply = self.total_supply + amount;

            self.env().emit_event(Mint {
                receiver,
                amount,
            });
            true
        }

        #[ink(message)]
        pub fn burn(&mut self, amount: u128, holder: AccountId) -> bool {
            self.ensure_owner_or_bridge(self.env().caller());

            let holder_balance = self.balance_of(holder.clone());
            if holder_balance < amount {
                return false;
            }

            self.balances.insert(holder.clone(), holder_balance - amount);
            self.total_supply = self.total_supply - amount;

            self.env().emit_event(Burn {
                holder,
                amount,
            });

            true
        }

        /// Transfers `value` amount of tokens from the caller's account to account `to`.
        ///
        /// On success a `Transfer` event is emitted.
        ///
        /// # Errors
        ///
        /// Returns `InsufficientBalance` error if there are not enough tokens on
        /// the caller's account balance.
        fn transfer_from_to(
            &mut self,
            from: AccountId,
            to: AccountId,
            value: Balance,
        ) -> bool {
            let from_balance = self.balance_of(from);
            if from_balance < value {
                return false;
            }
            self.balances.insert(from, from_balance - value);
            let to_balance = self.balance_of(to);
            self.balances.insert(to, to_balance + value);
            self.env().emit_event(Transfer {
                from: Some(from),
                to: Some(to),
                value,
            });
            true
        }

        fn ensure_owner_or_bridge(&self, caller: AccountId) {
            if self.bridge.is_some() {
                assert!(caller == self.bridge.unwrap() || caller == self.owner);
            } else {
                assert!(caller == self.owner);
            }
        }

        fn ensure_owner(&self, caller: AccountId) {
            assert!(caller == self.owner, "Only owner can call this function");
        }
    }

    /// Unit tests.
    #[cfg(test)]
    mod tests {
    }
}
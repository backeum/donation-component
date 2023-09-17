use crate::data::TrophyData;
use scrypto::prelude::*;

// function to generate the url for the image
pub fn generate_url(
    base_path: String,
    donated: Decimal,
    created: String,
    collection_id: String,
) -> String {
    format!(
        "{}/nft/collection/{}?donated={}&created={}",
        base_path, collection_id, donated, created
    )
}

// function to generate the created string with a date format
fn generate_created_string() -> String {
    let time = UtcDateTime::from_instant(&Clock::current_time_rounded_to_minutes()).unwrap();
    format!("{}-{}-{}", time.year(), time.month(), time.day_of_month())
}

#[blueprint]
mod collection {
    enable_method_auth! {
        roles {
            admin => updatable_by: [];
        },
        methods {
            donate_mint => PUBLIC;
            donate_update => PUBLIC;
            withdraw_donations => restrict_to: [admin];
        }
    }

    struct Collection {
        // Mints a proof that is used as proof of donated value to the NFT repository.
        trophy_resource_manager: ResourceManager,

        // NFT minter badge
        minter_badge: Vault,

        // Collected donations
        donations: Vault,

        // Specific user identity that owns this component
        user_identity: String,

        // Specific user name that owns this component
        user_name: String,

        // Specific user slug that owns this component
        user_slug: String,

        // Which collection this donation component is for
        collection_id: String,

        // Set the royalty amount on donations in this collection.
        royalty_amount: Decimal,
    }

    impl Collection {
        pub fn new(
            trophy_resource_manager: ResourceManager,
            owner_badge: ResourceAddress,
            royalty_amount: Decimal,
            minter_badge: Bucket,
            user_identity: String,
            user_name: String,
            user_slug: String,
            collection_id: String,
        ) -> (Global<Collection>, Bucket) {
            let domain: String = trophy_resource_manager
                .get_metadata("domain")
                .unwrap()
                .expect("No domain on NFT repository");

            // Creating an admin badge for the admin role, return it to the component creator.
            let admin_badge = ResourceBuilder::new_fungible(OwnerRole::None)
                .divisibility(DIVISIBILITY_NONE)
                .metadata(metadata!(
                    init {
                        "name" => "Admin Badge", locked;
                        "description" => "Used to manage your Backeum collection contract", locked;
                        "icon_url" => format!("{}{}", domain, "/bucket/assets/wallet-assets/admin-badge.png"), locked;
                        "tags" => vec!["backeum", "badge"], locked;
                        "info_url" => domain, locked;
                    }
                ))
                .mint_initial_supply(1);

            let component = Self {
                minter_badge: Vault::with_bucket(minter_badge),
                donations: Vault::new(XRD),
                user_identity,
                user_name,
                user_slug,
                collection_id,
                trophy_resource_manager,
                royalty_amount,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(rule!(require(owner_badge))))
            .enable_component_royalties(component_royalties! {
                roles {
                    royalty_setter => rule!(require(owner_badge));
                    royalty_setter_updater => rule!(deny_all);
                    royalty_locker => rule!(require(owner_badge));
                    royalty_locker_updater => rule!(deny_all);
                    royalty_claimer => rule!(require(owner_badge));
                    royalty_claimer_updater => rule!(deny_all);
                },
                init {
                    donate_mint => Xrd(royalty_amount), updatable;
                    donate_update => Xrd(royalty_amount), updatable;
                    withdraw_donations => Free, locked;
                }
            })
            .roles(roles!(
                admin => rule!(require(admin_badge.resource_address()));
            ))
            .globalize();

            (component, admin_badge.into())
        }

        // donate_mint is a public method, callable by anyone who want to donate to the user.
        pub fn donate_mint(&mut self, tokens: Bucket) -> Bucket {
            // Push a proof of minter badge to the local auth zone for minting a trophy.
            LocalAuthZone::push(self.minter_badge.as_fungible().create_proof_of_amount(1));

            // Get the domain name used from the trophy resource manager.
            let domain: String = self
                .trophy_resource_manager
                .get_metadata("domain")
                .unwrap()
                .expect("No domain on NFT repository");

            let created = generate_created_string();
            let mut data = TrophyData {
                name: format!("{}'s Trophy", self.user_name),
                info_url: format!("{}/p/{}", domain, self.user_slug),
                collection_id: self.collection_id.clone(),
                created: created.clone(),
                donated: dec!(0),
                key_image_url: "".to_string(),
            };

            let trophy = self
                .trophy_resource_manager
                .mint_ruid_non_fungible(data.clone());

            let nft_id = trophy
                .as_non_fungible()
                .non_fungible::<TrophyData>()
                .local_id()
                .clone();

            // Generate new data based on the updated donation value.
            data.donated += tokens.amount();
            data.donated += self.royalty_amount;
            data.key_image_url = generate_url(
                domain.to_string(),
                data.donated,
                data.created,
                self.collection_id.clone(),
            );

            // Update NF with new data
            self.trophy_resource_manager
                .update_non_fungible_data(&nft_id, "donated", data.donated);
            self.trophy_resource_manager.update_non_fungible_data(
                &nft_id,
                "key_image_url",
                data.key_image_url,
            );

            // Take all tokens, and return trophy.
            self.donations.put(tokens);
            trophy
        }

        // donate is a public method, callable by anyone who want to donate to the user.
        pub fn donate_update(&mut self, tokens: Bucket, proof: Proof) {
            LocalAuthZone::push(self.minter_badge.as_fungible().create_proof_of_amount(1));
            let domain: String = self
                .trophy_resource_manager
                .get_metadata("domain")
                .unwrap()
                .expect("No domain on NFT repository");

            // Check that the proof is of same resource address.
            let checked_proof = proof.check(self.trophy_resource_manager.address());

            // Retrieve the NF id from the proof, use it to update metadata on the NF.
            let nft_id = checked_proof.as_non_fungible().non_fungible_local_id();

            // Get data from the Trophy data based on NF id.
            let mut data: TrophyData = self.trophy_resource_manager.get_non_fungible_data(&nft_id);

            // Generate new data based on the updated donation value.
            data.donated += tokens.amount();
            data.donated += self.royalty_amount;
            data.key_image_url = generate_url(
                domain.to_string(),
                data.donated,
                data.created,
                self.collection_id.clone(),
            );

            // Update NF with new data
            self.trophy_resource_manager
                .update_non_fungible_data(&nft_id, "donated", data.donated);
            self.trophy_resource_manager.update_non_fungible_data(
                &nft_id,
                "key_image_url",
                data.key_image_url,
            );

            // Take all tokens, and return trophy.
            self.donations.put(tokens);
        }

        // withdraw_donations is a method for the admin to withdraw all donations.
        pub fn withdraw_donations(&mut self) -> Bucket {
            self.donations.take_all()
        }
    }
}

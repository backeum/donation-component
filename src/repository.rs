use crate::collection::collection::Collection;
use crate::data::Trophy;
use crate::util::*;
use scrypto::prelude::*;

#[blueprint]
#[types(Trophy)]
mod repository {
    enable_package_royalties! {
        new => Free;
        merge_trophies => Free;
        new_collection_component => Xrd(50.into());
        update_base_path => Free;
    }

    enable_method_auth! {
        roles {
            trophy_minter => updatable_by: [OWNER];
        },
        methods {
            new_collection_component => PUBLIC;
            merge_trophies => PUBLIC;
            update_base_path => restrict_to: [OWNER];
        }
    }

    struct Repository {
        // NFT resource address.
        trophy_resource_manager: ResourceManager,

        // Badge for being able to mint trophies.
        minter_badge_manager: ResourceManager,

        // The owner badge resource address used to set ownership of sub components.
        repository_owner_access_badge_address: ResourceAddress,

        // Dapp definition address
        dapp_definition_address: GlobalAddress,
    }

    impl Repository {
        pub fn new(
            base_path: String,
            repository_owner_access_badge_address: ResourceAddress,
            dapp_definition_address: GlobalAddress,
        ) -> Global<Repository> {
            let (address_reservation, component_address) =
                Runtime::allocate_component_address(Repository::blueprint_id());

            // Setup owner badge access rule
            let repository_owner_badge_access_rule: AccessRule =
                rule!(require(repository_owner_access_badge_address));

            // Creating an minter badge for the minter role. This is used to mint trophies both in
            // this blueprint and in the collection blueprint. The minter badge is handed down to
            // the collection blueprint via the factory method new_collection_component.
            let minter_badge_manager = ResourceBuilder::new_fungible(OwnerRole::Fixed(
                repository_owner_badge_access_rule.clone(),
            ))
            .divisibility(DIVISIBILITY_NONE)
            .metadata(metadata!(
                init {
                    "name" => "Trophies Minter", locked;
                    "description" => "Grants authorization to mint NFs from repository", locked;
                    "dapp_definition" => dapp_definition_address, locked;
                }
            ))
            .mint_roles(mint_roles! {
                minter => rule!(require(global_caller(component_address)));
                minter_updater => rule!(deny_all);
            })
            .withdraw_roles(withdraw_roles! {
                withdrawer => rule!(deny_all);
                withdrawer_updater => rule!(deny_all);
            })
            .create_with_no_initial_supply();

            // Manager for minting trophies for a central collection. This manager will be handed
            // down to collection components together with a minter badge. This allows all
            // collections to mint trophies from the same resource manager.
            let trophy_resource_manager = ResourceBuilder::new_ruid_non_fungible_with_registered_type::<Trophy>(OwnerRole::Fixed(repository_owner_badge_access_rule.clone()))
                .metadata(metadata!(
                    roles {
                        metadata_setter => rule!(require(global_caller(component_address)));
                        metadata_setter_updater => rule!(deny_all);
                        metadata_locker => rule!(deny_all);
                        metadata_locker_updater => rule!(deny_all);
                    },
                    init {
                        "name" => "Backeum Trophies", locked;
                        "description" => "Backeum trophies celebrates the patronage of its holder with donations to individual Backeum creators. A unique symbol of support for the community, it's a vibrant testament to financial encouragement.", locked;
                        "domain" => format!("{}", base_path), updatable;
                        "icon_url" => UncheckedUrl::of(format!("{}{}", base_path, "/bucket/assets/wallet-assets/trophy.png")), locked;
                        "tags" => vec!["backeum", "trophy"], locked;
                        "info_url" => UncheckedUrl::of(base_path), locked;
                        "dapp_definition" => dapp_definition_address, locked;
                    }
                ))
                .mint_roles(mint_roles!(
                    minter => rule!(require(minter_badge_manager.address()) || require(global_caller(component_address)));
                    minter_updater => rule!(require(global_caller(component_address)));
                ))
                .burn_roles(burn_roles!(
                    burner => rule!(require(global_caller(component_address)));
                    burner_updater => rule!(require(global_caller(component_address)));
                ))
                .non_fungible_data_update_roles(non_fungible_data_update_roles!(
                    non_fungible_data_updater => rule!(require(minter_badge_manager.address()) || require(global_caller(component_address)));
                    non_fungible_data_updater_updater => repository_owner_badge_access_rule.clone();
                ))
                .create_with_no_initial_supply();

            Self {
                trophy_resource_manager,
                minter_badge_manager,
                repository_owner_access_badge_address,
                dapp_definition_address,
            }
            .instantiate()
            .prepare_to_globalize(OwnerRole::Fixed(repository_owner_badge_access_rule.clone()))
            .metadata(metadata!(
                roles {
                    metadata_setter => rule!(require(repository_owner_access_badge_address));
                    metadata_setter_updater => rule!(deny_all);
                    metadata_locker => rule!(deny_all);
                    metadata_locker_updater => rule!(deny_all);
                },
                init {
                    "dapp_definition" => dapp_definition_address, locked;
                }
            ))
            .roles(roles! {
                trophy_minter => rule!(require(minter_badge_manager.address()));
            })
            .with_address(address_reservation)
            .globalize()
        }

        // new_collection_component sets up a new collection component for a user, and give that contract
        // a mint badge that allows for it to create and update trophies. By going through Repository
        // for instantiation we can ensure that the mint badge is only given to a contract that is
        // made by Backeum.
        pub fn new_collection_component(
            &mut self,
            user_name: String,
            user_slug: String,
            collection_owner_badge_proof: Proof,
        ) -> Global<Collection> {
            let mint_badge = self.minter_badge_manager.mint(1);
            Collection::new(
                self.trophy_resource_manager,
                self.repository_owner_access_badge_address,
                collection_owner_badge_proof.resource_address(),
                mint_badge,
                user_name,
                user_slug,
                self.dapp_definition_address,
            )
        }

        // update_base_path updates the base path for each trophy.
        pub fn update_base_path(
            &mut self,
            new_base_path: String,
            update_nft_ids: Vec<NonFungibleLocalId>,
        ) {
            self.trophy_resource_manager
                .set_metadata("domain", new_base_path.clone());

            for nft_id in update_nft_ids {
                // Get data from the Trophy data based on NF id.
                let mut data: Trophy = self.trophy_resource_manager.get_non_fungible_data(&nft_id);

                // Generate new image url.
                data.key_image_url = UncheckedUrl::of(generate_url(
                    new_base_path.to_string(),
                    data.donated,
                    data.created,
                    data.collection_id,
                ));

                // Update image url.
                self.trophy_resource_manager.update_non_fungible_data(
                    &nft_id,
                    "key_image_url",
                    data.key_image_url,
                );
            }
        }

        // merge_trophies will take multiple trophies of the same collection id and merge them into
        // one.
        pub fn merge_trophies(&mut self, trophies: Bucket) -> Bucket {
            assert_eq!(
                trophies.resource_address(),
                self.trophy_resource_manager.address(),
                "The given trophies is not the of the same resource type as managed by the repository."
            );

            let non_fungible_bucket = trophies.as_non_fungible();
            let trophies_list = non_fungible_bucket.non_fungibles::<Trophy>();
            let template = trophies_list.first().unwrap().data();
            let mut earliest_created: UtcDateTime =
                UtcDateTime::from_instant(&Clock::current_time_rounded_to_minutes()).unwrap();

            let mut donated = dec!(0);
            for trophy_data in trophies_list.iter() {
                assert_eq!(
                    trophy_data.data().collection_id,
                    template.collection_id,
                    "The given trophies is not the of the same collection id."
                );

                assert_eq!(
                    trophy_data.data().info_url,
                    template.info_url,
                    "The given trophies is not the of the same created date."
                );

                assert_eq!(
                    trophy_data.data().name,
                    template.name,
                    "The given trophies is not the of the same created date."
                );

                println!(
                    "Trophy created: {}",
                    parse_created_string(trophy_data.data().created)
                );

                let trophy_date = parse_created_string(trophy_data.data().created);

                if trophy_date
                    .to_instant()
                    .compare(earliest_created.to_instant(), TimeComparisonOperator::Lt)
                {
                    earliest_created = trophy_date;
                }

                donated += trophy_data.data().donated;
            }

            // Get the domain name used from the trophy resource manager.
            let domain: String = self
                .trophy_resource_manager
                .get_metadata("domain")
                .unwrap()
                .expect("No domain on NFT repository");

            let created = generate_created_string(earliest_created);
            let new_trophy_data = Trophy {
                name: template.name,
                info_url: template.info_url,
                collection_id: template.collection_id.clone(),
                created: created.clone(),
                donated,
                key_image_url: UncheckedUrl::of(generate_url(
                    domain.to_string(),
                    donated,
                    created.clone(),
                    template.collection_id.clone(),
                )),
            };

            // Burn the previous trophies.
            trophies.burn();

            self.trophy_resource_manager
                .mint_ruid_non_fungible(new_trophy_data.clone())
        }
    }
}

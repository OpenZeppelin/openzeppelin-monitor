# Changelog

## [1.0.0](https://github.com/OpenZeppelin/openzeppelin-monitor/compare/v0.2.0...v1.0.0) (2025-06-30)


### 🚀 Features

* add block tracker ([#11](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/11)) ([1d4d117](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1d4d117aab56e2c31c0747d6bf681fe60b2d8b10))
* Add CLA assistant bot ([#107](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/107)) ([47e490e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/47e490e4a5657a48bc60f85c38d72aca16334ac0))
* Add client rpc pool ([#75](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/75)) ([28cd940](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/28cd940a8aea5c97fb15a4ca0d415debaa2864b1))
* add email support ([#7](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/7)) ([decb56d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/decb56d45d3f1000346c24e137d1a5d952c4a9dd))
* Add endpoint rotation manager ([#69](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/69)) ([454a630](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/454a630cf92c305ea5d9254b211a7b60abf8804d))
* Add environment vars and Hashicorp cloud vault support (breaking) ([#199](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/199)) ([558304f](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/558304f335a645c1de2d348a041337ccba2c2a06))
* Add new error context ([#77](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/77)) ([612bb76](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/612bb76b9c8e9a470fc68685c2f06481663a9474))
* Add rc workflow file ([#156](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/156)) ([8907591](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/890759186570a64a9d0b0ef4dc9e512d0110d7a0))
* Add support for webhook, telegram, discord notifications ([#65](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/65)) ([829967d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/829967da45062dc22ffb0cb3376e68101a46b3e9))
* Enhance filter expression parsing and evaluation ([#222](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/222)) ([3cb0849](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/3cb084919b3d477f329a85fbafce1ce6d696b16d))
* Extend support for EVM transaction properties ([#187](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/187)) ([f20086b](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/f20086b0431a787dd55aa8928a09aece80b9a731))
* Handle Stellar JSON-RPC outside of retention window error for `getTransactions` and `getEvents` ([#270](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/270)) ([ae116ff](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ae116ff10f393a04c19d3b845df656027c6be4b9))
* Implement client pooling for Webhook-based notifiers ([#281](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/281)) ([4f480c6](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/4f480c6a05aeb949cfd8e227c5c08f19a5e60180))
* Introduce `TransportError` ([#259](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/259)) ([0e04cfb](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/0e04cfb57109251095ef8ee526fb5e05f5792792))
* Introduce centralized retryable HTTP client creation ([#273](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/273)) ([5f6edaf](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5f6edaf5deb77a5d9dfead52a162e923aad6a2ab))
* Leverage contract spec (SEP-48) for Stellar functions (breaking) ([#208](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/208)) ([5ebc2a4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5ebc2a441b9ac6ed66a0807cac2795af2ae5b1c8))
* Markdown for telegram, discord, slack and email ([#197](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/197)) ([791bf4b](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/791bf4b347d8cfe03ccd53e9797f179c15629a33))
* Plat 6187 write metrics to prometheus ([#95](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/95)) ([2dc08d5](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/2dc08d51670834f453498299937debfca67fa1b7))
* PLAT-6148 Adding post filter to monitor model ([#58](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/58)) ([920a0bf](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/920a0bf27953b67eb722d17d5ebf50b51237d4d4))
* PLAT-6151 Integrate custom script execution with notification service ([#79](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/79)) ([bd5f218](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/bd5f218507dfc30bd4b2182077e2997cf04b8877))
* PLAT-6477 Adding rust toolchain file ([#117](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/117)) ([ea6fb1e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ea6fb1ee6bba46cfa66a0c81665e17930bbbed93))
* Separate code test coverage into different categories of tests ([#84](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/84)) ([a3ad89c](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/a3ad89cdcf0bab5883af7ec36b854fedc2f060cd))
* spawn block-watcher per network ([#4](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/4)) ([d7a19ec](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/d7a19ec57344e4fb28dffc6f2025e809d0f5d946))
* Test execute the monitor against specific block ([#133](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/133)) ([563c34f](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/563c34fde3c0f334a7c5884de5510bf27e4fca48))


### 🐛 Bug Fixes

* Add thread flag when running tests in CI ([#41](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/41)) ([4312669](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/4312669d8da84f5cf7e7817b10c377fe3a6992af))
* Adding validation for unknown field names ([#223](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/223)) ([cadf4da](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/cadf4dac293e2c24a02a2eb188540e1eb312b75f))
* Adjust netlify toml settings ([#47](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/47)) ([af9fe55](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/af9fe553a92cfc47a306a7dcfc43be0b2257f835))
* CLA labels and assistant ([#176](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/176)) ([b14f060](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/b14f0600dc4cac5a5f00d3772328abe123114b2a))
* Docs link ([#106](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/106)) ([f12d95d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/f12d95d85ad9230bece0342c39cb5c3c1cd62832))
* Docs pipeline ([#167](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/167)) ([1e78ec4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1e78ec4f98f70ac12dea353c1605ac4ac2c5734b))
* Documentation name for antora ([#105](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/105)) ([5a8c4bd](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5a8c4bd8315e62bb2dedb066f6b6bfcaa09c2d37))
* Duplicate name in triggers config ([#274](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/274)) ([00f58f4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/00f58f4be3f9452792f9fdcf5dd8696947a274cb))
* Environment adjustments and cargo lock file improvements ([#219](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/219)) ([1b4d5d8](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1b4d5d8dbe8cba26fbb84a8f847fc22b1a1dc096))
* Event and function signatures from matched_on ([#198](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/198)) ([cdd9f1d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/cdd9f1d7333ee2f3ef9c476a08e918388b3c35f0))
* Fix cargo lock ([#110](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/110)) ([c440ca4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/c440ca43542e919cd473a7d533b0820cf5474d3e))
* Fix cargo lock file ([#116](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/116)) ([1bd3658](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1bd3658ab507c2dde90a2132b6eaec6d849e0e3c))
* Fix the codecov yaml syntax ([#97](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/97)) ([fcafcbf](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/fcafcbf5765014a65c3f2c8718ee0f24a4531ebe))
* fixed check ([1d36aaa](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1d36aaa63ca12b4a660ec7e7bfcb18f722d8adf2))
* Linter ([b0e27ca](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/b0e27ca21f8e39b3a3c16d356df00dfcd0a868e5))
* Monitor match template var signature collission (breaking) ([#203](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/203)) ([283b724](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/283b724a88f45f82c3c5fc81742a564b70909d45))
* Pagination logic in stellar getEvents relies only on cursor data ([#265](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/265)) ([fca4057](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/fca4057ff5847e04981e5903eebe6ccf3931726c))
* PLAT-6301 Remove logic for checking file descriptors open and fixing readme ([#90](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/90)) ([71dbd24](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/71dbd24a9ba5ab4c37cf4be432a4614c2e68166b))
* Reduce USDC ABI and fix trailing comma ([#62](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/62)) ([92e343c](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/92e343c09dc2da565912b6cd5bc83fbdc591cdb5))
* remove the create-github-app-token action from the scorecard workflow ([#174](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/174)) ([48ca0b1](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/48ca0b106dbee225b5d4824013c2a28b773b23b3))
* rename docker binaries ([#2](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/2)) ([78d438a](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/78d438a1ca4931651d3ca106c5dbda1ea1357574))
* rename import ([#6](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/6)) ([745e591](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/745e591faba06f557b2f6a091434250ed559df6e))
* Replace automatic minor version bumps ([#285](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/285)) ([0c9e14a](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/0c9e14a542cae2d2c7ff580ff7de28b0d9aab22a))
* Risk of key collision for monitor custom scripts ([#258](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/258)) ([2aa4cd7](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/2aa4cd730dbcbbd1cf0892394cedc4ea06332375))
* Running duplicate tests ([#181](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/181)) ([ad0f741](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ad0f741608b2719a1db16dd22bf8c457e5814f86))
* Stellar ledgers are deterministic ([#257](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/257)) ([56a9f9e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/56a9f9e10e533ea96c01cb1f0f67024600ad89df))
* trigger execution order ([#24](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/24)) ([26581fe](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/26581fec9ec1078ea4284fd6b43509616c66ad64))
* Variable resolving ([#49](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/49)) ([e26d173](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/e26d17314e9b2e78c0772a46f3139da70c6ca144))

## [0.2.0](https://github.com/OpenZeppelin/openzeppelin-monitor/compare/v0.1.0...v0.2.0) (2025-05-14)


## ⚠️ ⚠️ Breaking Changes in v0.2.0

* Renamed abi to contract_spec in monitor configurations.
* Stellar function expressions now use named parameters instead of positional indexes, for example;

    ```
    (Transfer(address,address,amount)):
    2 > 1000  → amount > 1000
    ```
* Template variables now follow dot notation rather than underscores, for example:
    * monitor_name → monitor.name
    * transaction_hash → transaction.hash
    * function_0_amount → functions.0.args.amount
    * event_0_signature → events.0.signature
* Sensitive configuration values (e.g., URLs, usernames, passwords, tokens) must now be defined using the SecretValue object structure, for example:

    * RPC URLs:

        ```
        "rpc_urls": [
            {
                "type_": "rpc",
                "url": {
                "type": "plain",
                "value": "https://eth.drpc.org"
                },
                "weight": 100
            }
        ]
        ```

    * Webhook URLs:

        ```
        "discord_url": {
            "type": "plain",
            "value": "https://discord.com/api/webhooks/123-456-789"
        }
        ```


### 🚀 Features

* Add environment vars and Hashicorp cloud vault support (breaking) ([#199](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/199)) ([558304f](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/558304f335a645c1de2d348a041337ccba2c2a06))
* Extend support for EVM transaction properties ([#187](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/187)) ([f20086b](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/f20086b0431a787dd55aa8928a09aece80b9a731))
* Leverage contract spec (SEP-48) for Stellar functions (breaking) ([#208](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/208)) ([5ebc2a4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5ebc2a441b9ac6ed66a0807cac2795af2ae5b1c8))
* Markdown for telegram, discord, slack and email ([#197](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/197)) ([791bf4b](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/791bf4b347d8cfe03ccd53e9797f179c15629a33))
* Test execute the monitor against specific block ([#133](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/133)) ([563c34f](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/563c34fde3c0f334a7c5884de5510bf27e4fca48))


### 🐛 Bug Fixes

* Adding validation for unknown field names ([#223](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/223)) ([cadf4da](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/cadf4dac293e2c24a02a2eb188540e1eb312b75f))
* CLA labels and assistant ([#176](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/176)) ([b14f060](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/b14f0600dc4cac5a5f00d3772328abe123114b2a))
* Docs pipeline ([#167](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/167)) ([1e78ec4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1e78ec4f98f70ac12dea353c1605ac4ac2c5734b))
* Environment adjustments and cargo lock file improvements ([#219](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/219)) ([1b4d5d8](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1b4d5d8dbe8cba26fbb84a8f847fc22b1a1dc096))
* Event and function signatures from matched_on ([#198](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/198)) ([cdd9f1d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/cdd9f1d7333ee2f3ef9c476a08e918388b3c35f0))
* Monitor match template var signature collission (breaking) ([#203](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/203)) ([283b724](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/283b724a88f45f82c3c5fc81742a564b70909d45))
* remove the create-github-app-token action from the scorecard workflow ([#174](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/174)) ([48ca0b1](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/48ca0b106dbee225b5d4824013c2a28b773b23b3))
* Running duplicate tests ([#181](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/181)) ([ad0f741](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ad0f741608b2719a1db16dd22bf8c457e5814f86))

## 0.1.0 (2025-04-07)


### 🚀 Features

* add block tracker ([#11](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/11)) ([1d4d117](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1d4d117aab56e2c31c0747d6bf681fe60b2d8b10))
* Add CLA assistant bot ([#107](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/107)) ([47e490e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/47e490e4a5657a48bc60f85c38d72aca16334ac0))
* Add client rpc pool ([#75](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/75)) ([28cd940](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/28cd940a8aea5c97fb15a4ca0d415debaa2864b1))
* add email support ([#7](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/7)) ([decb56d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/decb56d45d3f1000346c24e137d1a5d952c4a9dd))
* Add endpoint rotation manager ([#69](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/69)) ([454a630](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/454a630cf92c305ea5d9254b211a7b60abf8804d))
* Add new error context ([#77](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/77)) ([612bb76](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/612bb76b9c8e9a470fc68685c2f06481663a9474))
* Add rc workflow file ([#156](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/156)) ([8907591](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/890759186570a64a9d0b0ef4dc9e512d0110d7a0))
* Add support for webhook, telegram, discord notifications ([#65](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/65)) ([829967d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/829967da45062dc22ffb0cb3376e68101a46b3e9))
* Plat 6187 write metrics to prometheus ([#95](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/95)) ([2dc08d5](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/2dc08d51670834f453498299937debfca67fa1b7))
* PLAT-6148 Adding post filter to monitor model ([#58](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/58)) ([920a0bf](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/920a0bf27953b67eb722d17d5ebf50b51237d4d4))
* PLAT-6151 Integrate custom script execution with notification service ([#79](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/79)) ([bd5f218](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/bd5f218507dfc30bd4b2182077e2997cf04b8877))
* PLAT-6477 Adding rust toolchain file ([#117](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/117)) ([ea6fb1e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ea6fb1ee6bba46cfa66a0c81665e17930bbbed93))
* Separate code test coverage into different categories of tests ([#84](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/84)) ([a3ad89c](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/a3ad89cdcf0bab5883af7ec36b854fedc2f060cd))
* spawn block-watcher per network ([#4](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/4)) ([d7a19ec](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/d7a19ec57344e4fb28dffc6f2025e809d0f5d946))


### 🐛 Bug Fixes

* Add thread flag when running tests in CI ([#41](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/41)) ([4312669](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/4312669d8da84f5cf7e7817b10c377fe3a6992af))
* Adjust netlify toml settings ([#47](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/47)) ([af9fe55](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/af9fe553a92cfc47a306a7dcfc43be0b2257f835))
* Docs link ([#106](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/106)) ([f12d95d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/f12d95d85ad9230bece0342c39cb5c3c1cd62832))
* Documentation name for antora ([#105](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/105)) ([5a8c4bd](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5a8c4bd8315e62bb2dedb066f6b6bfcaa09c2d37))
* Fix cargo lock ([#110](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/110)) ([c440ca4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/c440ca43542e919cd473a7d533b0820cf5474d3e))
* Fix cargo lock file ([#116](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/116)) ([1bd3658](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1bd3658ab507c2dde90a2132b6eaec6d849e0e3c))
* Fix the codecov yaml syntax ([#97](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/97)) ([fcafcbf](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/fcafcbf5765014a65c3f2c8718ee0f24a4531ebe))
* fixed check ([1d36aaa](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/1d36aaa63ca12b4a660ec7e7bfcb18f722d8adf2))
* Linter ([b0e27ca](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/b0e27ca21f8e39b3a3c16d356df00dfcd0a868e5))
* Netlify integration & Release workflow doc ([#162](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/162)) ([3b77025](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/3b7702569e7c5828ca55fb67f7eec2672bf768b2))
* PLAT-6301 Remove logic for checking file descriptors open and fixing readme ([#90](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/90)) ([71dbd24](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/71dbd24a9ba5ab4c37cf4be432a4614c2e68166b))
* Reduce USDC ABI and fix trailing comma ([#62](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/62)) ([92e343c](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/92e343c09dc2da565912b6cd5bc83fbdc591cdb5))
* rename docker binaries ([#2](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/2)) ([78d438a](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/78d438a1ca4931651d3ca106c5dbda1ea1357574))
* rename import ([#6](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/6)) ([745e591](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/745e591faba06f557b2f6a091434250ed559df6e))
* trigger execution order ([#24](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/24)) ([26581fe](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/26581fec9ec1078ea4284fd6b43509616c66ad64))
* Variable resolving ([#49](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/49)) ([e26d173](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/e26d17314e9b2e78c0772a46f3139da70c6ca144))


### 📚 Documentation

* Add Antora documentation ([#48](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/48)) ([2f737c4](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/2f737c4c040090bd3acd0af90d3f24045b8ff173))
* add link to contributing in README ([#33](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/33)) ([5abb548](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5abb548c199f3a033860b027461e5fb3cd60e565))
* Add list of RPC calls ([#67](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/67)) ([aae9577](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/aae9577f4e011eaca12adb7997bf5fd28a558f83))
* Add quickstart guide ([#56](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/56)) ([e422353](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/e422353873335540afce5a9a5702c786c71eea75))
* add readme documentation ([#8](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/8)) ([357006d](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/357006d98f6cc8d160920e702dc78662008d39a3))
* add rust documentation ([#5](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/5)) ([3832570](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/3832570adf4854279fcda215fbbba5eb0d5396a1))
* Adding node to docker images - custom scripts ([#76](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/76)) ([da6516c](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/da6516c6f3afccb297cb1c1251f673e02ceaeaa5))
* Custom scripts documentation to antora and readme ([#91](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/91)) ([2b81058](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/2b81058f810e6b4d18a2c79e96002fb77890e9e0))
* Fix quickstart closing tag ([#118](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/118)) ([d360379](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/d3603796f39c15ed5247efab90ab95c5537c76d2))
* Fix telegram channel ([9899259](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/98992599ab8998113b6202781787a48ce0aab3db))
* Implement README feedback ([#50](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/50)) ([5b6ba64](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/5b6ba6419a06b9abd60412fa02b09da2a416e38c))
* Improve docs ([#100](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/100)) ([9586a25](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/9586a253f2a76993bbf82d4834b37863edabab60))
* improve readme section and examples ([#9](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/9)) ([009db37](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/009db3719e1be03120733755ade3c1c45e13f8a5))
* Improvements to custom scripts ([#98](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/98)) ([69047d9](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/69047d90a2fe057446f7c1b3f3526ab31bc6afcb))
* Re-order example and fix test flag ([#52](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/52)) ([f90b6df](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/f90b6df73ef7a6040eab59d71402b34877c88fc5))
* Readability improvements ([#109](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/109)) ([8e23389](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/8e23389ea0dcb3b221227a6cddd17de39603acbb))
* Update project structure ([#101](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/101)) ([207edd2](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/207edd28f3fb0a805d40d6ba9109abe9e6553d23))
* Update README and antora docs ([#57](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/57)) ([6a2299e](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/6a2299e0c41052ef9523aec1aa6f5852990e9179))
* Update RPC documentation after client pool feature ([#96](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/96)) ([ade2811](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/ade2811431c07c6b46730cbce5e357934df14cd5))
* Update telegram channel in docs ([#99](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/99)) ([9899259](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/98992599ab8998113b6202781787a48ce0aab3db))
* Updated Quickstart guide ([#108](https://github.com/OpenZeppelin/openzeppelin-monitor/issues/108)) ([b81c7cd](https://github.com/OpenZeppelin/openzeppelin-monitor/commit/b81c7cd22143a7d2854ef496ab59e114d70c360f))

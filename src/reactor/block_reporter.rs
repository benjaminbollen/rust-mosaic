// Copyright 2018 OpenST Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module is the implementation of block reporter reactor.

use futures::future::Either;
use futures::future::IntoFuture;
use futures::Future;
use rlp;
use std::sync::Arc;
use web3::contract::Contract;
use web3::contract::Options;
use web3::transports::Http;
use web3::types::Address;

use ethereum::types::block::Block;
use reactor::React;
use web3::contract::Error;

/// This is gas consumed by report block operation. This is calculated by estimate gas
/// function from web3.
const REPORT_BLOCK_ESTIMATED_GAS: i32 = 3_000_000;

pub struct BlockReporter {
    block_store: Arc<Contract<Http>>,
    from: Address,
}

impl BlockReporter {
    /// Creates a new instance of BlockReporter
    ///
    /// # Arguments
    ///
    /// * `block_store` - Contract instance of block store.
    /// * `from` - Address which does block reporting.
    pub fn new(block_store: Arc<Contract<Http>>, from: Address) -> Self {
        BlockReporter { block_store, from }
    }
}

impl React for BlockReporter {
    /// Defines how different reactor will react on block observation.
    ///
    /// # Arguments
    ///
    /// * `block` - The observed block.
    /// * `event_loop` - The reactor's event loop to handle the tasks spawned.
    fn react(&self, block: &Block, event_loop: &tokio_core::reactor::Handle) {
        info!("Reporting block for number {:?} ", block.number);

        let encoded_block = rlp::encode(block);
        let block_hash = block.hash();
        let call_future = self
            .block_store
            .query(
                "isBlockReported",
                block_hash,
                self.from,
                Options::default(),
                None,
            ).then({
                let block_store_contract = Arc::clone(&self.block_store);
                let from = self.from.clone();
                move |result: Result<bool, web3::contract::Error>| match result {
                    Ok(is_reported) => if is_reported {
                        Either::A(Ok(()).into_future())
                    } else {
                        Either::B(
                            block_store_contract
                                .call(
                                    "reportBlock",
                                    encoded_block,
                                    from,
                                    Options::with(|opt| {
                                        opt.gas = Some(REPORT_BLOCK_ESTIMATED_GAS.into())
                                    }),
                                ).then(move |tx| {
                                    info!("Block reported got tx: {:?}", tx);
                                    Ok(())
                                }).map_err(|error: Error| {
                                    error!("Error reporting block {:?}", error)
                                }),
                        )
                    },
                    Err(error) => {
                        error!(
                            "Error while checking if block is already reported{:?}",
                            error
                        );
                        // Event loop spawn expects certain types. It doesn't support err types.
                        Either::A(Ok(()).into_future())
                    }
                }
            });
        event_loop.spawn(call_future);
    }
}

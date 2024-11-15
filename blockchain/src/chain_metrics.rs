use nimiq_block::{Block, EquivocationProof};
use nimiq_blockchain_interface::{ChunksPushError, ChunksPushResult, PushError, PushResult};
use nimiq_hash::Blake2bHash;
use prometheus_client::{
    encoding::{EncodeLabelSet, EncodeLabelValue},
    metrics::{counter::Counter, family::Family},
    registry::Registry,
};

#[derive(Default)]
pub struct BlockchainMetrics {
    block_push_counts: Family<PushResultLabels, Counter>,
    transactions_counts: Family<TransactionProcessedLabels, Counter>,
    skip_blocks: Counter,
    equivocation_counts: Family<EquivocationProofLabels, Counter>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct PushResultLabels {
    push_result: BlockPushResult,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
enum BlockPushResult {
    Known,
    Extended,
    Rebranched,
    Forked,
    Ignored,
    Orphan,
    Invalid,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct TransactionProcessedLabels {
    ty: TransactionProcessed,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
enum TransactionProcessed {
    Applied,
    Reverted,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct EquivocationProofLabels {
    ty: EquivocationProofType,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
enum EquivocationProofType {
    Fork,
    DoubleVote,
    DoubleProposal,
}

impl BlockchainMetrics {
    pub fn register(&self, registry: &mut Registry) {
        registry.register(
            "block_push_counts",
            "Count of block push results",
            self.block_push_counts.clone(),
        );

        registry.register(
            "transaction_counts",
            "Count of transactions applied/reverted",
            self.transactions_counts.clone(),
        );

        registry.register(
            "skip_blocks",
            "Number of skip blocks applied",
            self.skip_blocks.clone(),
        );

        registry.register(
            "equivocation_counts",
            "Count of equivocation proofs applied",
            self.equivocation_counts.clone(),
        );
    }

    #[inline]
    pub fn note_push_result(
        &self,
        push_result: &Result<(PushResult, Result<ChunksPushResult, ChunksPushError>), PushError>,
    ) {
        let push_result = match push_result {
            Ok((PushResult::Known, _)) => BlockPushResult::Known,
            Ok((PushResult::Extended, _)) => BlockPushResult::Extended,
            Ok((PushResult::Rebranched, _)) => BlockPushResult::Rebranched,
            Ok((PushResult::Forked, _)) => BlockPushResult::Forked,
            Ok((PushResult::Ignored, _)) => BlockPushResult::Ignored,
            Err(PushError::Orphan) => BlockPushResult::Orphan,
            Err(_) => {
                self.note_invalid_block();
                return;
            }
        };
        self.block_push_counts
            .get_or_create(&PushResultLabels { push_result })
            .inc();
    }

    #[inline]
    pub fn note_invalid_block(&self) {
        self.block_push_counts
            .get_or_create(&PushResultLabels {
                push_result: BlockPushResult::Invalid,
            })
            .inc();
    }

    #[inline]
    pub fn note_extend(&self, block: &Block) {
        self.transactions_counts
            .get_or_create(&TransactionProcessedLabels {
                ty: TransactionProcessed::Applied,
            })
            .inc_by(block.num_transactions() as u64);

        if block.is_skip() {
            self.skip_blocks.inc();
        }

        if block.is_micro() && block.has_body() {
            let body = block.unwrap_micro_ref().body.as_ref().unwrap();
            for equivocation in &body.equivocation_proofs {
                let ty = match equivocation {
                    EquivocationProof::Fork(_) => EquivocationProofType::Fork,
                    EquivocationProof::DoubleVote(_) => EquivocationProofType::DoubleVote,
                    EquivocationProof::DoubleProposal(_) => EquivocationProofType::DoubleProposal,
                };
                self.equivocation_counts
                    .get_or_create(&EquivocationProofLabels { ty })
                    .inc();
            }
        }
    }

    #[inline]
    pub fn note_rebranch(
        &self,
        reverted_blocks: &[(Blake2bHash, Block)],
        adopted_blocks: &[(Blake2bHash, Block)],
    ) {
        for (_, block) in reverted_blocks {
            if block.is_micro() {
                self.transactions_counts
                    .get_or_create(&TransactionProcessedLabels {
                        ty: TransactionProcessed::Reverted,
                    })
                    .inc_by(block.num_transactions() as u64);
            }
        }

        for (_, block) in adopted_blocks {
            self.note_extend(block);
        }
    }
}

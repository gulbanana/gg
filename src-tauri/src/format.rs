// imitates the output of commit_templater, which is mostly private

use std::cmp::max;

use jj_lib::{
    backend::{ChangeId, CommitId},
    hex_util::to_reverse_hex,
    id_prefix::IdPrefixContext,
    object_id::ObjectId,
    repo::Repo,
};

pub enum CommitOrChangeId {
    Commit(CommitId),
    Change(ChangeId),
}

pub struct ShortestIdPrefix {
    pub prefix: String,
    pub rest: String,
}

impl CommitOrChangeId {
    pub fn hex(&self) -> String {
        match self {
            CommitOrChangeId::Commit(id) => id.hex(),
            CommitOrChangeId::Change(id) => to_reverse_hex(&id.hex()).unwrap(),
        }
    }

    pub fn shortest(
        &self,
        repo: &dyn Repo,
        id_prefix_context: &IdPrefixContext,
        total_len: usize,
    ) -> ShortestIdPrefix {
        let mut hex = self.hex();
        let prefix_len = match self {
            CommitOrChangeId::Commit(id) => id_prefix_context.shortest_commit_prefix_len(repo, id),
            CommitOrChangeId::Change(id) => id_prefix_context.shortest_change_prefix_len(repo, id),
        };
        hex.truncate(max(prefix_len, total_len));
        let rest = hex.split_off(prefix_len);
        ShortestIdPrefix { prefix: hex, rest }
    }
}

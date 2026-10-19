//! Child of the roster service: all private snapshot identities stay native.
use super::*;
use crate::{uniform_changes as u, uniform_transaction as tx};
pub(super) struct UniformAccess<'a> {
    writer: blacktop_native::UniformWriter,
    expected: tx::Owner,
    stop: &'a AtomicBool,
    started: Instant,
}
impl<'a> UniformAccess<'a> {
    pub(super) fn prepare(
        native: &NativeSnapshot,
        game: &Path,
        edit: &u::Edit,
        cached_team: &Value,
        cached_uniform: &Value,
        stop: &'a AtomicBool,
    ) -> Result<(Self, tx::Owner, u::Plan)> {
        use tx::Access;
        ensure!(!stop.load(Ordering::Acquire), "Roster service is closing");
        let meta = native
            .teams
            .as_ref()
            .context("No native uniform snapshot; refresh teams")?;
        meta.directory.validate()?;
        let team = meta
            .teams
            .get(edit.record_index)
            .context("Team is absent from this native snapshot")?;
        let address = *team
            .uniform_slots
            .get(edit.slot)
            .context("Uniform slot is absent from this native snapshot")?;
        let table = crate::team_changes::uint(&meta.uniform_header, 8, 8)?;
        let count = usize::try_from(crate::team_changes::uint(&meta.uniform_header, 0, 8)?)?;
        ensure!(
            count == meta.uniforms.len()
                && (1..=8192).contains(&count)
                && address >= table
                && (address - table).is_multiple_of(u::STRIDE as u64),
            "The team slot has no exact native uniform record"
        );
        let index = usize::try_from((address - table) / u::STRIDE as u64)?;
        let identity = meta
            .uniforms
            .get(index)
            .context("Uniform slot is outside the native directory")?;
        ensure!(
            team.id == edit.team_id
                && identity.id == edit.uniform_id
                && identity.team_id == edit.team_id
                && identity.filename == edit.expected_base_filename
                && cached_uniform["recordIndex"].as_u64() == Some(index as u64),
            "The uniform identity differs from this tab's snapshot"
        );
        ensure!(
            gate(&session_safety::processes()?, game)? == native.game,
            "Game differs from this editor snapshot"
        );
        let reader = Reader::open_roster(&native.game, game)?;
        let m = Bounded {
            reader: &reader,
            stop,
            started: Instant::now(),
        };
        VerifiedLayout::verify(&m, native.table)?;
        let team_address = meta.directory.team_table
            + (edit.record_index * crate::team_changes::TEAM_STRIDE) as u64;
        let team_raw = read_large(&m, team_address, crate::team_changes::TEAM_STRIDE)?;
        let uniform_raw = m.read(address, u::STRIDE)?;
        ensure!(
            team_raw == read_large(&m, team_address, crate::team_changes::TEAM_STRIDE)?
                && uniform_raw == m.read(address, u::STRIDE)?,
            "Uniform or team values changed during inspection"
        );
        let plan = u::plan(
            edit,
            cached_team,
            cached_uniform,
            &team_raw,
            &uniform_raw,
            address,
        )?;
        let refs: Vec<_> = meta.teams.iter().map(|t| t.uniform_slots.clone()).collect();
        let slots = team
            .uniform_slots
            .iter()
            .flat_map(|p| p.to_le_bytes())
            .collect::<Vec<_>>();
        let expected = tx::Owner {
            game: native.game.clone(),
            image: native.image,
            root: native.root,
            owner: native.owner,
            allocation_base: native.allocation.base,
            allocation_len: native.allocation.len,
            player_table: native.table,
            player_count: native.count,
            team_table: meta.directory.team_table,
            team_count: meta.directory.team_count,
            arena_table: meta.directory.arena_table,
            arena_count: meta.directory.arena_count,
            uniform_table: table,
            uniform_count: count,
            index: edit.record_index,
            team_id: edit.team_id,
            team_name: edit.expected_team_name.clone(),
            slot: edit.slot,
            uniform_index: index,
            uniform_id: identity.id,
            filename: identity.filename.clone(),
            slots_hash: crate::team_changes::hash(&slots),
            references_hash: tx::reference_hash(&refs, address),
            serializer: roster_snapshot::serializer_fingerprint().into(),
        };
        expected.validate()?;
        let writer = blacktop_native::UniformWriter::open(&native.game, game, address)?;
        VerifiedLayout::verify(writer.reader(), native.table)?;
        let result = Self {
            writer,
            expected: expected.clone(),
            stop,
            started: Instant::now(),
        };
        ensure!(
            result.owner()? == expected && result.capture()? == plan.before,
            "The uniform owner or values changed before Save"
        );
        Ok((result, expected, plan))
    }
}
impl tx::Access for UniformAccess<'_> {
    fn owner(&self) -> Result<tx::Owner> {
        let m = self.writer.reader();
        let e = &self.expected;
        let root = pointer(m, m.base() + 0x9A47EB8)?;
        let owner = pointer(m, root + 0xA8)?;
        let ph = m.read(owner + 0x10, 16)?;
        let th = m.read(e.allocation_base + 0xD0, 16)?;
        let ah = m.read(e.allocation_base + 0xB0, 16)?;
        let uh = m.read(e.allocation_base + 0x290, 16)?;
        let mut result = e.clone();
        result.game = m.identity.clone();
        result.image = m.base();
        result.root = root;
        result.owner = owner;
        result.player_table = crate::team_changes::uint(&ph, 8, 8)?;
        result.player_count = crate::team_changes::uint(&ph, 0, 4)? as usize;
        let (base, len) = m.roster_region(result.player_table)?;
        result.allocation_base = base;
        result.allocation_len = len;
        result.team_table = crate::team_changes::uint(&th, 8, 8)?;
        result.team_count = usize::try_from(crate::team_changes::uint(&th, 0, 8)?)?;
        result.arena_table = crate::team_changes::uint(&ah, 8, 8)?;
        result.arena_count = usize::try_from(crate::team_changes::uint(&ah, 0, 8)?)?;
        result.uniform_table = crate::team_changes::uint(&uh, 8, 8)?;
        result.uniform_count = usize::try_from(crate::team_changes::uint(&uh, 0, 8)?)?;
        ensure!(
            result.game == e.game
                && result.image == e.image
                && result.root == e.root
                && result.owner == e.owner
                && result.allocation_base == e.allocation_base
                && result.allocation_len == e.allocation_len
                && result.player_table == e.player_table
                && result.player_count == e.player_count
                && result.team_table == e.team_table
                && result.team_count == e.team_count
                && result.arena_table == e.arena_table
                && result.arena_count == e.arena_count
                && result.uniform_table == e.uniform_table
                && result.uniform_count == e.uniform_count,
            "Uniform database or directories changed"
        );
        let team = read_large(
            m,
            e.team_table + (e.index * crate::team_changes::TEAM_STRIDE) as u64,
            crate::team_changes::TEAM_STRIDE,
        )?;
        let team_fields = crate::team_changes::fields(&team)?;
        result.team_id = team_fields["teamId"].as_u64().context("Team ID missing")? as u16;
        result.team_name = team_fields["name"]
            .as_str()
            .context("Team name missing")?
            .into();
        ensure!(
            crate::team_changes::uint(&team, 0xDE0 + e.slot * 8, 8)? == e.address(),
            "Uniform slot no longer owns this record"
        );
        result.slots_hash = crate::team_changes::hash(&team[0xDE0..0xEE0]);
        let identity = m.read(e.address(), 0x54)?;
        result.uniform_id = crate::team_changes::uint(&identity, 0x50, 2)? as u16;
        ensure!(
            crate::team_changes::uint(&identity, 0x52, 2)? == e.team_id as u64,
            "Uniform team identity changed"
        );
        result.filename = u::ascii(&identity, 8, 64)?;
        let refs = (0..e.team_count)
            .map(|i| {
                let bytes = m.read(
                    e.team_table + (i * crate::team_changes::TEAM_STRIDE + 0xDE0) as u64,
                    256,
                )?;
                Ok(bytes
                    .chunks_exact(8)
                    .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                    .collect::<Vec<_>>())
            })
            .collect::<Result<Vec<_>>>()?;
        result.references_hash = tx::reference_hash(&refs, e.address());
        ensure!(
            ph == m.read(owner + 0x10, 16)?
                && th == m.read(e.allocation_base + 0xD0, 16)?
                && ah == m.read(e.allocation_base + 0xB0, 16)?
                && uh == m.read(e.allocation_base + 0x290, 16)?
                && root == pointer(m, m.base() + 0x9A47EB8)?
                && owner == pointer(m, root + 0xA8)?,
            "Uniform directory changed during inspection"
        );
        Ok(result)
    }
    fn capture(&self) -> Result<Vec<u8>> {
        let m = self.writer.reader();
        let raw = m.read(self.expected.address(), u::STRIDE)?;
        ensure!(
            raw == m.read(self.expected.address(), u::STRIDE)?,
            "Uniform values changed during inspection"
        );
        Ok(raw)
    }
    fn write(&self, offset: usize, bytes: &[u8]) -> Result<()> {
        self.writer.write(offset, bytes)
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Uniform save cancelled because the launcher is closing"
        );
        ensure!(
            self.started.elapsed() < Duration::from_secs(12),
            "Uniform save exceeded its time budget"
        );
        Ok(())
    }
}

//! Native read-only APPEARANCEDATA ownership; no process discovery/writes.
use super::*;
const HEADER: usize = 0x3d0;
fn bytes(m: &impl Inspect, at: u64, n: usize) -> Result<Vec<u8>> {
    span(at, n)?;
    ensure!(n <= 4096, "Appearance source read bound");
    m.checkpoint()?;
    let b = m.read(at, n)?;
    ensure!(b.len() == n, "Partial appearance source read");
    Ok(b)
}
fn qword(m: &impl Inspect, at: u64) -> Result<u64> {
    Ok(u64::from_le_bytes(bytes(m, at, 8)?.as_slice().try_into()?))
}
fn private(m: &impl Inspect, at: u64, n: usize, allocation: u64) -> Result<()> {
    span(at, n)?;
    let end = at + n as u64;
    let mut p = at;
    let mut count = 0;
    while p < end {
        count += 1;
        ensure!(count <= 4096, "Appearance source region bound");
        let r = m.region(p)?;
        ensure!(
            r.committed
                && r.kind == 0x20000
                && r.allocation == allocation
                && [2, 4, 8].contains(&r.protection)
                && r.start <= p
                && r.len > 0,
            "Appearance source allocation changed"
        );
        let next = r
            .start
            .checked_add(r.len)
            .context("Appearance region overflow")?;
        ensure!(next > p, "Appearance region progress");
        p = end.min(next);
    }
    Ok(())
}
#[derive(Clone, PartialEq, Eq)]
struct Header {
    raw: Vec<u8>,
    appearance: u64,
    pool: u64,
    pool_count: usize,
    person: u64,
    person_count: usize,
}
fn header(m: &impl Inspect, o: &Owner) -> Result<Header> {
    o.validate()?;
    ensure!(
        o.kind == Kind::Player && m.base() == o.image,
        "Appearance source owner differs"
    );
    verify_profile(m)?;
    ensure!(
        qword(m, o.image + 0x9ace338)? == o.root
            && qword(m, o.root)? == o.image + 0x74ebe98
            && qword(m, o.root + 0xa8)? == o.owner,
        "Appearance source root changed"
    );
    private(m, o.owner, HEADER, o.region_base)?;
    let raw = bytes(m, o.owner, HEADER)?;
    let q = |i: usize| u64::from_le_bytes(raw[i..i + 8].try_into().unwrap());
    let d = |i: usize| u32::from_le_bytes(raw[i..i + 4].try_into().unwrap()) as usize;
    ensure!(
        q(0x10) == o.count as u64 && q(0x18) == o.table,
        "Appearance player directory changed"
    );
    let mut spans = Vec::new();
    for (offset, stride, min, max, is32) in [
        (0x10, 0x4f8, 100, 8192, false),
        (0x60, 0x1278, 1, 2048, false),
        (0x80, 0x1550, 1, 1024, false),
        (0x290, 0x170, 1, 8192, false),
        (0x180, 0x1c8, 1, 8192, true),
        (0x3c0, 0x68, 1, 65536, true),
    ] {
        let count = if is32 {
            d(offset)
        } else {
            usize::try_from(q(offset))?
        };
        let at = q(offset + 8);
        if offset == 0x290 && count == 0 && at == 0 {
            continue;
        }
        ensure!(
            (min..=max).contains(&count),
            "Appearance typed directory count changed"
        );
        let n = count
            .checked_mul(stride)
            .context("Appearance directory overflow")?;
        ensure!(
            at % 8 == 0
                && at >= o.region_base + 0x680
                && at
                    .checked_add(n as u64)
                    .is_some_and(|e| e <= o.region_base + o.region_len)
                && !overlap(at, n, o.owner, HEADER)
                && !overlap(at, n, o.root, 0xb0)
                && spans.iter().all(|&(p, z)| !overlap(at, n, p, z)),
            "Appearance typed directory aliases or exceeds owner"
        );
        private(m, at, n, o.region_base)?;
        spans.push((at, n));
    }
    let appearance = qword(m, o.table + o.index as u64 * 0x4f8 + 0x78)?;
    let pool = q(0x3c8);
    let pool_count = d(0x3c0);
    ensure!(
        appearance >= pool
            && (appearance - pool) % 0x68 == 0
            && (appearance - pool) / 0x68 < (pool_count as u64),
        "Appearance is not a native pool member"
    );
    private(m, appearance, 0x68, o.region_base)?;
    let r = m.region(appearance)?;
    ensure!(
        r.protection == 4 && appearance % 4 == 0,
        "Appearance atomic region changed"
    );
    Ok(Header {
        appearance,
        pool,
        pool_count,
        person: q(0x188),
        person_count: d(0x180),
        raw,
    })
}
fn member(h: &Header, p: u64) -> bool {
    p == 0 || (p >= h.pool && (p - h.pool) % 0x68 == 0 && (p - h.pool) / 0x68 < h.pool_count as u64)
}
fn projection(m: &impl Inspect, o: &Owner, h: &Header) -> Result<(Vec<u64>, Vec<u64>)> {
    let mut players = Vec::with_capacity(o.count);
    for i in 0..o.count {
        let p = qword(m, o.table + i as u64 * 0x4f8 + 0x78)?;
        ensure!(
            member(h, p) && (i == o.index || p != h.appearance),
            "Appearance PLAYER pool alias changed"
        );
        players.push(p);
    }
    let mut people = Vec::with_capacity(h.person_count);
    for i in 0..h.person_count {
        let p = qword(m, h.person + i as u64 * 0x1c8)?;
        ensure!(
            member(h, p) && p != h.appearance,
            "Appearance PERSON pool alias changed"
        );
        people.push(p);
    }
    Ok((players, people))
}
fn seal(m: &impl Inspect, o: &Owner, h: &Header) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for v in [o.image, o.root, o.owner, o.region_base, o.region_len] {
        hash.update(v.to_le_bytes());
    }
    hash.update(&h.raw);
    hash.update(h.appearance.to_le_bytes());
    hash.update(bytes(m, h.appearance + 4, 100)?);
    Ok(hash.finalize().into())
}
pub(crate) fn verify_pool_profile(m: &impl Inspect) -> Result<()> {
    let proof: Value = serde_json::from_str(include_str!(
        "../assets/appearance-v2-production-proof.json"
    ))?;
    ensure!(
        proof["schema"] == "nba2k27-v2-native-appearance-cas-proof-v1",
        "Appearance native proof changed"
    );
    for method in proof["methods"]
        .as_array()
        .context("Missing native appearance pins")?
    {
        let rva = method["rva"]
            .as_u64()
            .context("Missing native appearance RVA")?;
        let n = method["bytes"]
            .as_u64()
            .context("Missing native appearance span")? as usize;
        ensure!((1..=4096).contains(&n), "Appearance code bound");
        let at = m
            .base()
            .checked_add(rva)
            .context("Appearance code overflow")?;
        let r = m.region(at)?;
        ensure!(
            r.committed
                && r.kind == 0x1000000
                && r.allocation == m.base()
                && r.protection == 0x20
                && r.start <= at
                && at
                    .checked_add(n as u64)
                    .is_some_and(|e| e <= r.start + r.len),
            "Appearance code protection changed"
        );
        ensure!(
            format!("{:x}", Sha256::digest(bytes(m, at, n)?))
                == method["sha256"].as_str().unwrap_or(""),
            "Appearance native method changed"
        );
    }
    Ok(())
}
pub(crate) fn generation(m: &impl Inspect, o: &Owner) -> Result<Generation> {
    verify_pool_profile(m)?;
    let a = header(m, o)?;
    projection(m, o, &a)?;
    let stamp = seal(m, o, &a)?;
    ensure!(
        header(m, o)? == a,
        "Appearance owner header changed during capture"
    );
    let start = o.region_base;
    let end = start + o.region_len;
    let mut known = vec![(o.root, 0xb0)];
    if a.appearance > start {
        known.push((start, (a.appearance - start) as usize));
    }
    if a.appearance + 4 < end {
        known.push((a.appearance + 4, (end - a.appearance - 4) as usize));
    }
    Generation::from_native(o.clone(), stamp, known)
}
pub(crate) fn person_pointers(m: &impl Inspect, o: &Owner) -> Result<Vec<u64>> {
    let a = header(m, o)?;
    let (_, people) = projection(m, o, &a)?;
    ensure!(header(m, o)? == a, "Appearance person header changed");
    Ok(people)
}
pub(crate) fn metadata(m: &impl Inspect, g: &Generation, at: u64) -> Result<[u8; 32]> {
    let a = header(m, &g.owner)?;
    ensure!(at == a.appearance, "Appearance metadata address changed");
    let value = seal(m, &g.owner, &a)?;
    ensure!(
        header(m, &g.owner)? == a && value == g.stamp,
        "Appearance metadata generation changed"
    );
    Ok(value)
}

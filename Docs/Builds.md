# Deployed Builds, 2025-10-01 to 2026-11-11

Responsibility: lists the upstream OpenFront builds (record `gitCommit`) observed in public game records, with the engine facts that matter for the simulator.

## Method

- Data: the public API (`/public/games`, `/public/game/<id>`). For every UTC day from 2025-10-01 to 2026-11-11 two windows (06:00-06:20 and 18:00-18:20) were listed, and in each the shortest Public and the shortest non-Public (Singleplayer) game were fetched. Builds with fewer than 3 records got extra samples (shortest games within +-6 h of their sightings).
- Held-out day 2026-10-02 (UTC) was never fetched (guarded in the scripts; 0 such records). Hence a build deployed only that day would be invisible.
- 2088 records, 159 distinct builds. Records are stored outside git in `Cache/Records/<commit12>/<gameId>.json.gz`, index in `Cache/Records/Index.csv`.
- Tags: exact tags pointing at the commit (`git tag --points-at`); otherwise the nearest tag (`git describe`) and the first tag containing it. "On main" is `git merge-base --is-ancestor <commit> origin/main` (release-branch tags are usually not ancestors of main).
- PRNG / DetMath: read from the commit's tree (`PseudoRandom.ts` mentions sfc32 or seedrandom; `DetMath.ts` exists). File paths moved in the monorepo split (upstream e82286146), so the tree was searched by file name.
- Columns: records = total (Public/Other; FFA/Team).

## Findings

- Every sampled build is a tagged upstream release (158 of 159; one untagged commit, `a26585a47b17`, seen once). Betas and a test-release were played publicly too (`v0.26.0-beta1..4`, `v0.27.0-beta2`, `v0.29.0-beta1..4`, `v0.30.0-beta1/2/5`, `v0.32.0-beta1/3/5/6`, `v0.34.0-test-release`).
- Builds are not strictly sequential in time: on many days two or more builds produce Public games (e.g. 2025-10-04 to 2025-10-14 alternate `bc5f18dee882`, `fd6b0ee062e2`, `136cfa1316dd`, 06:00 vs 18:00 windows differ on the same day), also with rollbacks (e.g. `4c9fcdedf448` reappears on 2025-11-12..18 after `82ff4e43c1e1`). Singleplayer records carry the commit of the client build, which may lag or lead the server. The simulator must therefore select the engine by each record's own `gitCommit`, never by date.
- Many builds live only hours to a few days (15 builds have a single sample, 1 has two); the list is a lower bound of deployed builds, not complete.
- Release cadence rises sharply from 2026-09 (v0.34.0 on 2026-09-14, v0.34.x almost daily).
- PRNG: old algorithm is `seedrandom` (the npm package, default ARC4 stream cipher, seeded with `String(seed)`), used up to upstream 2e6f70c09 (PR #4233, 2026-06-12), after which `PseudoRandom` is inline sfc32 with splitmix32 seeding and 12 warm-up draws. First sfc32 build seen: `5be72db060fc` (2026-06-15, v0.32.0-beta5).
- DetMath (deterministic `exp/log/pow/atan2`) exists from upstream f2c9a5378 (PR #5191, 2026-08-30). First build seen with it: `1e973bb534b8` (2026-09-14, v0.34.0). Builds without it use native `Math.*` (platform dependent, so bit-exact replay of older transcendental results needs care).
- Counts: seedrandom without DetMath: 88 builds; sfc32 without DetMath: 36; sfc32 with DetMath: 35.
- Baseline `4e837520e886` (v0.34.22) is first seen on 2026-10-03; 2026-10-02 is held out.

## Builds (ordered by first day seen)

| commit12 | tag(s) | first day | last day | records | PRNG | DetMath | notes |
|---|---|---|---|---|---|---|---|
| `30856341cdd0` | v0.25.20 | 2025-10-01 | 2025-10-01 | 4 (2P/2O; 3F/1T) | seedrandom | no |  |
| `bc5f18dee882` | v0.25.21 | 2025-10-02 | 2025-10-13 | 82 (11P/71O; 68F/14T) | seedrandom | no |  |
| `fa9259b1fc58` | v0.26.0-beta1 | 2025-10-04 | 2025-10-04 | 1 (1P/0O; 0F/1T) | seedrandom | no | short-lived |
| `fd6b0ee062e2` | v0.26.0-beta2 | 2025-10-05 | 2025-10-11 | 9 (8P/1O; 5F/4T) | seedrandom | no |  |
| `a26585a47b17` | (v0.25.16+1, first in v0.25.17) | 2025-10-06 | 2025-10-06 | 1 (1P/0O; 1F/0T) | seedrandom | no | untagged; short-lived |
| `136cfa1316dd` | v0.26.0-beta3 | 2025-10-12 | 2025-10-14 | 4 (4P/0O; 2F/2T) | seedrandom | no |  |
| `f016c3ee9d90` | v0.25.23 | 2025-10-14 | 2025-10-14 | 3 (1P/2O; 2F/1T) | seedrandom | no |  |
| `2ad9c6f302a2` | v0.26.5 | 2025-10-15 | 2025-10-18 | 3 (2P/1O; 2F/1T) | seedrandom | no |  |
| `f552b006613c` | v0.26.6 | 2025-10-15 | 2025-10-24 | 60 (17P/43O; 40F/20T) | seedrandom | no |  |
| `69fc14f1fd4d` | v0.26.0-beta4 | 2025-10-17 | 2025-10-17 | 1 (1P/0O; 1F/0T) | seedrandom | no | short-lived |
| `4c9fcdedf448` | v0.26.7 | 2025-10-25 | 2025-11-18 | 14 (10P/4O; 5F/9T) | seedrandom | no |  |
| `0a2b6ed9a6a9` | v0.26.9 | 2025-10-27 | 2025-10-28 | 7 (3P/4O; 4F/3T) | seedrandom | no |  |
| `4f00c4a12cb7` | v0.26.11 | 2025-10-29 | 2025-10-30 | 8 (4P/4O; 4F/4T) | seedrandom | no |  |
| `6fd1576d7b38` | v0.26.13 | 2025-10-31 | 2025-11-04 | 20 (10P/10O; 10F/10T) | seedrandom | no |  |
| `1b113873d60c` | v0.26.15 | 2025-11-05 | 2025-11-07 | 10 (5P/5O; 5F/5T) | seedrandom | no |  |
| `822d08bb7cc4` | v0.26.14 | 2025-11-05 | 2025-11-05 | 3 (1P/2O; 2F/1T) | seedrandom | no |  |
| `82ff4e43c1e1` | v0.26.16 | 2025-11-08 | 2025-11-15 | 27 (12P/15O; 18F/9T) | seedrandom | no |  |
| `824e1d61f5a4` | v0.26.17 | 2025-11-15 | 2025-11-22 | 25 (10P/15O; 17F/8T) | seedrandom | no |  |
| `599342995c1c` | v0.27.0-beta2 | 2025-11-20 | 2025-11-21 | 3 (3P/0O; 1F/2T) | seedrandom | no |  |
| `5fbdea3c3965` | v0.27.0 | 2025-11-23 | 2025-12-15 | 12 (10P/2O; 8F/4T) | seedrandom | no |  |
| `d88a6ba88753` | v0.27.1 | 2025-11-24 | 2025-12-03 | 39 (19P/20O; 19F/20T) | seedrandom | no |  |
| `2b606f280b86` | v0.27.2 | 2025-12-04 | 2025-12-07 | 13 (5P/8O; 9F/4T) | seedrandom | no |  |
| `cb4cf091ffd0` | v0.27.3 | 2025-12-08 | 2025-12-08 | 3 (1P/2O; 0F/3T) | seedrandom | no |  |
| `50fe3581cc0a` | v0.27.6 | 2025-12-09 | 2025-12-11 | 9 (4P/5O; 3F/6T) | seedrandom | no |  |
| `afa3e70b6231` | v0.27.5 | 2025-12-09 | 2025-12-09 | 3 (1P/2O; 2F/1T) | seedrandom | no |  |
| `dab04eddb465` | v0.27.8 | 2025-12-12 | 2025-12-12 | 3 (1P/2O; 2F/1T) | seedrandom | no |  |
| `2e52c0aa4193` | v0.27.10 | 2025-12-13 | 2025-12-18 | 22 (10P/12O; 15F/7T) | seedrandom | no |  |
| `f4ed2a932fa0` | v0.28.0 | 2025-12-19 | 2025-12-23 | 20 (10P/10O; 13F/7T) | seedrandom | no |  |
| `a810e0ad346d` | v0.28.1 | 2025-12-24 | 2025-12-25 | 8 (4P/4O; 4F/4T) | seedrandom | no |  |
| `671c9ea4ac9d` | v0.28.2 | 2025-12-26 | 2025-12-31 | 24 (12P/12O; 12F/12T) | seedrandom | no |  |
| `ec694593a64f` | v0.28.3 | 2026-01-01 | 2026-01-01 | 4 (2P/2O; 2F/2T) | seedrandom | no |  |
| `793efe4dd1ba` | v0.28.4 | 2026-01-02 | 2026-01-06 | 20 (10P/10O; 10F/10T) | seedrandom | no |  |
| `e0ecad9cb83e` | v0.28.5 | 2026-01-07 | 2026-01-07 | 4 (2P/2O; 2F/2T) | seedrandom | no | not on main |
| `d93b71da9475` | v0.28.7 | 2026-01-08 | 2026-01-19 | 112 (17P/95O; 94F/18T) | seedrandom | no | not on main |
| `e24628987635` | v0.29.0-beta1 | 2026-01-13 | 2026-01-13 | 2 (1P/1O; 2F/0T) | seedrandom | no | short-lived |
| `24774b480454` | v0.29.0-beta2 | 2026-01-15 | 2026-01-15 | 1 (1P/0O; 0F/1T) | seedrandom | no | short-lived |
| `9cd87f8906f3` | v0.29.0-beta3 | 2026-01-16 | 2026-01-16 | 1 (1P/0O; 1F/0T) | seedrandom | no | short-lived |
| `c6021ab38e9f` | v0.29.0-beta4 | 2026-01-17 | 2026-01-21 | 6 (6P/0O; 5F/1T) | seedrandom | no |  |
| `776c644a84ce` | v0.29.2 | 2026-01-20 | 2026-01-20 | 3 (0P/3O; 2F/1T) | seedrandom | no | no Public record |
| `f6454963b215` | v0.29.1 | 2026-01-20 | 2026-01-20 | 3 (1P/2O; 3F/0T) | seedrandom | no |  |
| `e9e2f06d69c6` | v0.29.3 | 2026-01-21 | 2026-01-24 | 15 (7P/8O; 15F/0T) | seedrandom | no |  |
| `87dab71e42b7` | v0.29.4 | 2026-01-25 | 2026-01-26 | 8 (4P/4O; 7F/1T) | seedrandom | no |  |
| `2984bec4d1b6` | v0.29.5 | 2026-01-27 | 2026-01-28 | 8 (4P/4O; 8F/0T) | seedrandom | no |  |
| `1eed561be5bc` | v0.29.10 | 2026-01-29 | 2026-01-30 | 8 (4P/4O; 7F/1T) | seedrandom | no |  |
| `6c2e0d1528b8` | v0.29.11 | 2026-01-31 | 2026-02-01 | 8 (4P/4O; 8F/0T) | seedrandom | no |  |
| `106938c395bb` | v0.29.12 | 2026-02-02 | 2026-02-03 | 7 (3P/4O; 7F/0T) | seedrandom | no |  |
| `dec926c54c75` | v0.29.7 | 2026-02-03 | 2026-02-18 | 3 (3P/0O; 2F/1T) | seedrandom | no |  |
| `d358ef79de8b` | v0.29.13 | 2026-02-04 | 2026-02-04 | 4 (2P/2O; 4F/0T) | seedrandom | no |  |
| `b55f7dadbbcb` | v0.29.14 | 2026-02-05 | 2026-02-14 | 38 (19P/19O; 38F/0T) | seedrandom | no |  |
| `97d0a05d58e9` | v0.29.15 | 2026-02-13 | 2026-02-13 | 3 (1P/2O; 3F/0T) | seedrandom | no |  |
| `a2a8035b1de1` | v0.29.16 | 2026-02-15 | 2026-02-25 | 38 (17P/21O; 38F/0T) | seedrandom | no |  |
| `339ace0bd677` | v0.30.0-beta1 | 2026-02-24 | 2026-02-25 | 3 (3P/0O; 3F/0T) | seedrandom | no |  |
| `7f03072e9bf9` | v0.29.17 | 2026-02-25 | 2026-02-26 | 5 (2P/3O; 4F/1T) | seedrandom | no |  |
| `1f05e2227767` | v0.29.18 | 2026-02-27 | 2026-03-03 | 20 (10P/10O; 19F/1T) | seedrandom | no |  |
| `55c5c260df18` | v0.29.20 | 2026-03-04 | 2026-03-09 | 47 (11P/36O; 46F/1T) | seedrandom | no |  |
| `df5fad1a1f6d` | v0.30.0-beta2 | 2026-03-09 | 2026-03-09 | 1 (1P/0O; 1F/0T) | seedrandom | no | short-lived |
| `2f7b41c53d0c` | v0.29.21 | 2026-03-10 | 2026-03-12 | 29 (6P/23O; 29F/0T) | seedrandom | no |  |
| `4cfefdfb0227` | v0.30.0 | 2026-03-12 | 2026-03-12 | 1 (0P/1O; 1F/0T) | seedrandom | no | short-lived; no Public record |
| `ebe7f76cd312` | v0.30.1 | 2026-03-12 | 2026-03-13 | 33 (1P/32O; 33F/0T) | seedrandom | no |  |
| `741a38c62b29` | v0.30.0-beta5 | 2026-03-13 | 2026-03-13 | 1 (1P/0O; 0F/1T) | seedrandom | no | short-lived |
| `5fb7f75f3d5b` | v0.30.6 | 2026-03-14 | 2026-03-14 | 3 (1P/2O; 1F/2T) | seedrandom | no |  |
| `da6969e7266c` | v0.30.7 | 2026-03-14 | 2026-03-17 | 7 (4P/3O; 6F/1T) | seedrandom | no |  |
| `90822fc52251` | v0.30.9 | 2026-03-16 | 2026-03-16 | 4 (2P/2O; 3F/1T) | seedrandom | no |  |
| `dd2c239aa166` | v0.30.10 | 2026-03-17 | 2026-03-17 | 3 (1P/2O; 2F/1T) | seedrandom | no |  |
| `3e7088cb2a2b` | v0.30.11 | 2026-03-18 | 2026-03-18 | 4 (2P/2O; 4F/0T) | seedrandom | no |  |
| `c5dad1f85055` | v0.30.12 | 2026-03-19 | 2026-03-21 | 12 (6P/6O; 10F/2T) | seedrandom | no |  |
| `481d0fa3a176` | v0.30.13 | 2026-03-22 | 2026-03-24 | 12 (6P/6O; 12F/0T) | seedrandom | no |  |
| `d809c25d1cdd` | v0.30.15 | 2026-03-25 | 2026-03-30 | 24 (12P/12O; 24F/0T) | seedrandom | no |  |
| `e7b431771888` | v0.30.18 | 2026-03-31 | 2026-03-31 | 4 (2P/2O; 3F/1T) | seedrandom | no |  |
| `f2507c38979c` | v0.30.22 | 2026-04-01 | 2026-04-01 | 4 (2P/2O; 4F/0T) | seedrandom | no |  |
| `930ff9f673d5` | v0.30.23 | 2026-04-02 | 2026-04-02 | 4 (2P/2O; 3F/1T) | seedrandom | no |  |
| `724d6390116c` | v0.30.24 | 2026-04-03 | 2026-04-07 | 18 (9P/9O; 17F/1T) | seedrandom | no |  |
| `592dadf80df6` | v0.30.25 | 2026-04-07 | 2026-04-16 | 38 (19P/19O; 33F/5T) | seedrandom | no |  |
| `1509bc9f13c9` | v0.30.27 | 2026-04-17 | 2026-04-21 | 18 (9P/9O; 17F/1T) | seedrandom | no |  |
| `b32b81c86c9e` | v0.30.26 | 2026-04-17 | 2026-04-17 | 3 (1P/2O; 3F/0T) | seedrandom | no |  |
| `de54f7c1a161` | v0.30.28 | 2026-04-22 | 2026-05-03 | 49 (23P/26O; 44F/5T) | seedrandom | no |  |
| `5d6c48ebf23f` | v0.31.0 | 2026-05-03 | 2026-05-03 | 3 (0P/3O; 3F/0T) | seedrandom | no | no Public record |
| `ac679b68c5a8` | v0.31.1 | 2026-05-03 | 2026-05-03 | 3 (1P/2O; 3F/0T) | seedrandom | no |  |
| `257fb9b38ec2` | v0.31.3 | 2026-05-04 | 2026-05-04 | 21 (1P/20O; 21F/0T) | seedrandom | no |  |
| `853216f9659c` | v0.31.6 | 2026-05-04 | 2026-05-04 | 6 (0P/6O; 6F/0T) | seedrandom | no | not on main; no Public record |
| `c63d3f8a2363` | v0.31.5 | 2026-05-04 | 2026-05-04 | 1 (1P/0O; 1F/0T) | seedrandom | no | not on main; short-lived |
| `ffbe48ad106b` | v0.31.8 | 2026-05-05 | 2026-05-05 | 4 (2P/2O; 4F/0T) | seedrandom | no |  |
| `af5249655c56` | v0.31.9 | 2026-05-06 | 2026-05-08 | 12 (6P/6O; 11F/1T) | seedrandom | no |  |
| `a3f695ee54b5` | v0.31.10 | 2026-05-09 | 2026-05-28 | 12 (7P/5O; 10F/2T) | seedrandom | no |  |
| `96e8f5cccd9f` | v0.31.11 | 2026-05-11 | 2026-05-12 | 4 (2P/2O; 4F/0T) | seedrandom | no |  |
| `aeb8d60224e3` | v0.31.12 | 2026-05-12 | 2026-06-17 | 218 (68P/150O; 204F/14T) | seedrandom | no | not on main |
| `26d8a314aec4` | v0.32.0-beta1 | 2026-06-08 | 2026-06-08 | 1 (1P/0O; 0F/1T) | seedrandom | no | short-lived |
| `f17ca9bd6551` | v0.32.0-beta3 | 2026-06-10 | 2026-06-10 | 1 (1P/0O; 1F/0T) | seedrandom | no | short-lived |
| `5be72db060fc` | v0.32.0-beta5 | 2026-06-15 | 2026-06-15 | 1 (1P/0O; 0F/1T) | sfc32 | no | short-lived |
| `661d96ba280f` | v0.32.0 | 2026-06-17 | 2026-06-17 | 3 (0P/3O; 3F/0T) | sfc32 | no | no Public record |
| `c9fe71070003` | v0.32.1 | 2026-06-18 | 2026-06-18 | 4 (2P/2O; 3F/1T) | sfc32 | no | not on main |
| `d7b781f155dc` | v0.32.2 | 2026-06-19 | 2026-06-22 | 16 (8P/8O; 16F/0T) | sfc32 | no | not on main |
| `bb2f2f01ee88` | v0.32.3 | 2026-06-22 | 2026-06-25 | 35 (5P/30O; 33F/2T) | sfc32 | no | not on main |
| `305534cc6579` | v0.32.0-beta6 | 2026-06-23 | 2026-06-23 | 1 (1P/0O; 1F/0T) | sfc32 | no | short-lived |
| `b22f422728f3` | v0.32.4 | 2026-06-26 | 2026-06-27 | 6 (3P/3O; 6F/0T) | sfc32 | no | not on main |
| `0c4c7d7993c9` | v0.32.5 | 2026-06-27 | 2026-07-01 | 18 (9P/9O; 15F/3T) | sfc32 | no | not on main |
| `580460c9692a` | v0.32.6 | 2026-07-02 | 2026-07-03 | 8 (4P/4O; 8F/0T) | sfc32 | no | not on main |
| `53e1a5b03e35` | v0.32.7 | 2026-07-04 | 2026-07-09 | 22 (11P/11O; 19F/3T) | sfc32 | no | not on main |
| `3687eee03bec` | v0.32.8 | 2026-07-09 | 2026-07-10 | 6 (3P/3O; 6F/0T) | sfc32 | no | not on main |
| `dcc18d5231af` | v0.32.9 | 2026-07-11 | 2026-07-16 | 24 (12P/12O; 23F/1T) | sfc32 | no | not on main |
| `16be9d7c15d7` | v0.32.10 | 2026-07-17 | 2026-07-17 | 4 (2P/2O; 4F/0T) | sfc32 | no | not on main |
| `fe5d7708e03a` | v0.32.12 | 2026-07-18 | 2026-07-18 | 4 (2P/2O; 2F/2T) | sfc32 | no | not on main |
| `b716adb7e2f1` | v0.32.13 | 2026-07-19 | 2026-07-21 | 10 (5P/5O; 10F/0T) | sfc32 | no | not on main |
| `f0da41820727` | v0.32.14 | 2026-07-21 | 2026-07-23 | 33 (4P/29O; 33F/0T) | sfc32 | no | not on main |
| `e3676439d216` | v0.32.11 | 2026-07-22 | 2026-07-22 | 1 (1P/0O; 1F/0T) | sfc32 | no | not on main; short-lived |
| `7d1c2edfb68e` | v0.32.15 | 2026-07-24 | 2026-07-24 | 9 (1P/8O; 8F/1T) | sfc32 | no | not on main |
| `efa4dadeb6f6` | v0.32.16 | 2026-07-24 | 2026-07-24 | 3 (1P/2O; 3F/0T) | sfc32 | no | not on main |
| `dd1277e245b5` | v0.32.17 | 2026-07-25 | 2026-08-03 | 44 (19P/25O; 40F/4T) | sfc32 | no | not on main |
| `7a7ca5be8ff8` | v0.33.0 | 2026-08-03 | 2026-08-03 | 3 (1P/2O; 2F/1T) | sfc32 | no |  |
| `dcbfdbbdc914` | v0.33.1 | 2026-08-04 | 2026-08-07 | 16 (8P/8O; 10F/6T) | sfc32 | no | not on main |
| `e9e10703e818` | v0.33.2 | 2026-08-08 | 2026-08-10 | 12 (6P/6O; 10F/2T) | sfc32 | no | not on main |
| `20c813f06a40` | v0.33.4 | 2026-08-11 | 2026-08-14 | 14 (7P/7O; 12F/2T) | sfc32 | no | not on main |
| `ad765842bac4` | v0.33.3 | 2026-08-11 | 2026-08-11 | 3 (1P/2O; 3F/0T) | sfc32 | no | not on main |
| `87f1a5278c8e` | v0.33.5 | 2026-08-15 | 2026-08-19 | 20 (10P/10O; 15F/5T) | sfc32 | no | not on main |
| `bebc953804e5` | v0.33.6 | 2026-08-20 | 2026-08-21 | 8 (4P/4O; 8F/0T) | sfc32 | no | not on main |
| `2d5baafdd0cc` | v0.33.7 | 2026-08-22 | 2026-08-22 | 3 (1P/2O; 2F/1T) | sfc32 | no | not on main |
| `d53d6c339fef` | v0.33.8 | 2026-08-22 | 2026-08-22 | 3 (1P/2O; 3F/0T) | sfc32 | no | not on main |
| `3229956f09a0` | v0.33.9 | 2026-08-23 | 2026-08-24 | 6 (3P/3O; 6F/0T) | sfc32 | no | not on main |
| `90513c0bffeb` | v0.33.10 | 2026-08-24 | 2026-08-24 | 3 (1P/2O; 2F/1T) | sfc32 | no | not on main |
| `0cb90ccb7478` | v0.33.11 | 2026-08-25 | 2026-08-28 | 13 (6P/7O; 10F/3T) | sfc32 | no | not on main |
| `115da0327634` | test-release,v0.34.0-test-release | 2026-08-28 | 2026-08-28 | 1 (1P/0O; 1F/0T) | sfc32 | no | short-lived |
| `88cc95d8b6d7` | v0.33.12 | 2026-08-28 | 2026-09-03 | 50 (13P/37O; 47F/3T) | sfc32 | no | not on main |
| `8b45be57542f` | v0.33.13 | 2026-09-04 | 2026-09-07 | 16 (8P/8O; 12F/4T) | sfc32 | no | not on main |
| `577819ba0e1e` | v0.33.14 | 2026-09-08 | 2026-09-14 | 33 (14P/19O; 28F/5T) | sfc32 | no | not on main |
| `1e973bb534b8` | v0.34.0 | 2026-09-14 | 2026-09-15 | 3 (1P/2O; 1F/2T) | sfc32 | yes |  |
| `a33efb780c4d` | v0.34.1 | 2026-09-15 | 2026-09-15 | 3 (1P/2O; 2F/1T) | sfc32 | yes | not on main |
| `129cfcb1c3e7` | v0.34.4 | 2026-09-16 | 2026-09-16 | 6 (1P/5O; 5F/1T) | sfc32 | yes | not on main |
| `222e4078982e` | v0.34.3 | 2026-09-16 | 2026-09-16 | 6 (1P/5O; 6F/0T) | sfc32 | yes | not on main |
| `adfffd3ab650` | v0.34.5 | 2026-09-16 | 2026-09-16 | 3 (0P/3O; 3F/0T) | sfc32 | yes | no Public record |
| `27611ec0b209` | v0.34.7 | 2026-09-17 | 2026-09-17 | 3 (1P/2O; 3F/0T) | sfc32 | yes |  |
| `ece79bfd3348` | v0.34.8 | 2026-09-17 | 2026-09-17 | 3 (1P/2O; 2F/1T) | sfc32 | yes |  |
| `2be7dfe239ea` | v0.34.10 | 2026-09-18 | 2026-09-18 | 4 (2P/2O; 3F/1T) | sfc32 | yes |  |
| `82e5fce9502b` | v0.34.11 | 2026-09-19 | 2026-09-20 | 8 (4P/4O; 7F/1T) | sfc32 | yes |  |
| `7c27263390d8` | v0.34.12 | 2026-09-21 | 2026-09-22 | 5 (2P/3O; 4F/1T) | sfc32 | yes | not on main |
| `cfa6c47a1a58` | v0.34.13 | 2026-09-22 | 2026-09-22 | 3 (1P/2O; 3F/0T) | sfc32 | yes | not on main |
| `e910d727d2dd` | v0.34.14 | 2026-09-22 | 2026-09-22 | 3 (1P/2O; 2F/1T) | sfc32 | yes |  |
| `59da4bc1bfaa` | v0.34.17 | 2026-09-23 | 2026-09-24 | 6 (3P/3O; 6F/0T) | sfc32 | yes | not on main |
| `a76a11b837e8` | v0.34.16 | 2026-09-23 | 2026-09-23 | 3 (1P/2O; 3F/0T) | sfc32 | yes | not on main |
| `824d1412be58` | v0.34.18 | 2026-09-25 | 2026-09-27 | 5 (2P/3O; 3F/2T) | sfc32 | yes | not on main |
| `a188868bc16c` | v0.34.19 | 2026-09-26 | 2026-09-26 | 3 (1P/2O; 3F/0T) | sfc32 | yes | not on main |
| `feba4a51c334` | v0.34.20 | 2026-09-26 | 2026-09-29 | 13 (7P/6O; 10F/3T) | sfc32 | yes | not on main |
| `d7075bface21` | v0.34.21 | 2026-09-30 | 2026-10-01 | 8 (4P/4O; 8F/0T) | sfc32 | yes | not on main |
| `4e837520e886` | v0.34.22 | 2026-10-03 | 2026-10-05 | 11 (5P/6O; 9F/2T) | sfc32 | yes | not on main |
| `71154c52c2a2` | v0.34.23 | 2026-10-05 | 2026-10-06 | 5 (3P/2O; 4F/1T) | sfc32 | yes | not on main |
| `3f5633b92c95` | v0.34.24 | 2026-10-07 | 2026-10-13 | 26 (13P/13O; 23F/3T) | sfc32 | yes | not on main |
| `dfd9e478b64d` | v0.34.25 | 2026-10-13 | 2026-10-16 | 18 (6P/12O; 15F/3T) | sfc32 | yes | not on main |
| `dfd420fd7836` | v0.35.0 | 2026-10-16 | 2026-10-16 | 3 (1P/2O; 3F/0T) | sfc32 | yes |  |
| `08b63c054897` | v0.35.2 | 2026-10-17 | 2026-10-23 | 28 (14P/14O; 24F/4T) | sfc32 | yes | not on main |
| `a2d20f8e5f32` | v0.35.3 | 2026-10-24 | 2026-10-24 | 4 (2P/2O; 3F/1T) | sfc32 | yes | not on main |
| `14ba82badb1e` | v0.35.4 | 2026-10-25 | 2026-10-26 | 8 (4P/4O; 8F/0T) | sfc32 | yes | not on main |
| `bf0ac8a1163f` | v0.35.5 | 2026-10-27 | 2026-10-27 | 4 (2P/2O; 4F/0T) | sfc32 | yes | not on main |
| `9f4a28f16fae` | v0.35.6 | 2026-10-28 | 2026-10-29 | 8 (4P/4O; 8F/0T) | sfc32 | yes | not on main |
| `c144d134d150` | v0.35.7 | 2026-10-30 | 2026-10-30 | 4 (2P/2O; 4F/0T) | sfc32 | yes | not on main |
| `7ee96860c717` | v0.35.9 | 2026-10-31 | 2026-11-04 | 20 (10P/10O; 17F/3T) | sfc32 | yes | not on main |
| `80bff198b68f` | v0.35.11 | 2026-11-05 | 2026-11-05 | 4 (2P/2O; 3F/1T) | sfc32 | yes | not on main |
| `1eec6bbceeec` | v0.35.12 | 2026-11-06 | 2026-11-06 | 4 (2P/2O; 4F/0T) | sfc32 | yes | not on main |
| `09aa423bceb9` | v0.35.14 | 2026-11-07 | 2026-11-09 | 16 (5P/11O; 13F/3T) | sfc32 | yes | not on main |
| `0211f17b71ef` | v0.36.0 | 2026-11-09 | 2026-11-09 | 3 (1P/2O; 2F/1T) | sfc32 | yes |  |
| `bc3e3334c573` | v0.36.1 | 2026-11-10 | 2026-11-11 | 8 (4P/4O; 6F/2T) | sfc32 | yes | not on main |

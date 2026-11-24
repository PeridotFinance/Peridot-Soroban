# v0 leverage loop: Mainnet results (24 Nov 2026)

Team account T1 `GA7X3VGKFRLQXZCJUOAXDBGM7NDEW7XGB47YNIJENKMIPUUD5F7U6OOJ` (Ledger HD 1). Live cash controller
CBMRS7…JRK (f41e5260), receipts 5630a701, receipt MarginController hook unset. Aquarius swaps with 1 % slippage bound.
Every transaction: SUCCESS. Explorer: `https://stellar.expert/explorer/public/tx/<hash>`.

## Summary

| Position | Opened at | HF at open | Liquidation | Close | Round-trip cost |
|---|---|---|---|---|---|
| Long XLM (XLM collateral, USDC debt) | 1.70x | 1.335 | XLM falls to $0.0952 | debt 0 | equity $6.60 → $6.53 (~1 %, swaps + XLM price move) |
| Short XLM (USDC collateral, XLM debt) | 2.00x | 1.500 | XLM rises to $0.1899 | debt 0 | ~$16.54 → ~$16.52 total wallet value (~0.1 %) |

Health factor never went below 1.25 on any step (non-atomic loop rule).
Final T1 state: 0 collateral, 0 debt, wallet 60.0036 XLM + 8.9239 USDC.

## Setup

| Step | Hash |
|---|---|
| l01 fund T1 (admin) | 08020e686ec5a0f47518e647fcc082e57fe26d77b9bc6c36b27305da49a27473 |
| l02 T1 USDC trustline | 2fe95af3b101dcd3bf3d92393b405ce45b76b12484293728f1be4ec8f7648ae7 |

## Long open (1.70x)

| Step | Hash |
|---|---|
| l10 deposit XLM | 6a1cf1c814627a66edfcca96a2212a01e470af4df02539ce49db06b249720ac8 |
| l11 borrow USDC | 69fab4d20bb022276c7d5abeb401fdd0b6301439728156872760adb0fc3c9b54 |
| l12 swap USDC→XLM | d0b1869ed5d2c6eb735a3c802b80c54dc05e02ceb2f6b8efdd55bad4361947f8 |
| l13 deposit XLM | 9823cb74b15e160df3d8d46f0e6c048e6544e41a24cd8578f1994043b2016698 |
| l14 borrow USDC | 79d05e4dd2b9b65ed96b145878c715e3705ee3109495201a2455df79fcdd93f8 |
| l15 swap | 93e48f09a163467cb381c2509ba90fd4bf6aee48eac3fe15a534cc5089cd7513 |
| l16 deposit | e1e5c8ac7e450036e8ac85c237a15e6256856e969d3231ad18aaede0d49980d7 |
| l17 borrow | d1e01b4a2d13a1c65a94fc2f1cca79973da8c7c098ba33d933ea4b07e7191b02 |
| l18 swap | d2fb1bfbba5c900e097f79f902b6872b8677b700f78ba732be9aeb6918c4d273 |
| l19 deposit | 7b021b330c0a31b539009ebc48b63e2556d0fe0771223e350898b66b1ec3ffc2 |

## Long close

| Step | Hash |
|---|---|
| l20 withdraw XLM | 3ca9c74d042ff8c1c84a7ed71098666aaf84ce4957024980c8653b7a5ac483a5 |
| l21 swap XLM→USDC | c3be0b61efcea02240d9eb51435ee660dc54f55b32943b420385eb35f02b89d0 |
| l22 repay USDC | 9cc114f98e277e8896fa583513a5ca686d1caec300b037c5f7f28a38717eec51 |
| l23 withdraw | 9d955f8019d0790a6e003d6abf03089afc589d901a29de06bd6f756379b2389d |
| l24 swap | 308645f1c08aa099125dfad5db6e0de811860633f339210a10989c514e64a1da |
| l25 repay | a1dce2cbe9645cc6ac0528e2f98b715763f1a253625d788a28cf9fd05ccde6d2 |
| l26 withdraw | 754f8f9db2fc512b5b45096a434e5a3304c464929c0c49878e23960af62262bd |
| l27 swap | ec4a532614c8f5d12f853c443c5b22dc6c3808e89df8712647b1c622fc9fb7d6 |
| l28 repay | 612d328577e34359e8ed08a16c37f629bdf3de7bdc2bdd4b03ae03c19244dc62 |
| l29 repay interest dust | 3094cb7a496541217cd6866c55de3c413988ac4fe37d8ed93478d01aa4b7a2d6 |
| l30 withdraw all XLM | 34552ff2295ffaa622d8df11595794cac7de9b17621368fc95897b208518ec95 |

## Short open (2.00x)

| Step | Hash |
|---|---|
| l40 pay T1 9 USDC (admin) | cf5f75e79ac8f2bd386566068aedf58fe7037cd80e2314304e39ef71848e713e |
| l41 deposit USDC | 1263f0c68eb73c983fc664b1616cf839c6eee22da2daa41c60006ca808d56afc |
| l42 borrow 42.83 XLM | 750d0b0de20ea9eb65713cf61e9b998b47075af6b064775a4876d09cd476fb3c |
| l43 swap XLM→USDC | 57392c4d9417be07ea34312919277c50e242d15a083af4ed3f96b849ea0e6741 |
| l44 deposit USDC | 1d732d45b11c541c5a511a13fe8dc0d0f0f4b98644bb5fa018c7632ffec1d449 |
| l45 borrow 25.71 XLM | 4dde82ad3a142f546bc405a513f8c293c9e2a72be957209ca435a03c021b1de4 |
| l46 swap | 367c7366889d4bf4ed06c23ac342010111790ac65458ce71a7bd2671f68cc644 |
| l47 deposit | e45a305330d37ae20d05952e0673831a938f8bb52e901bba54a7856e0e201205 |
| l48 borrow 2.79 XLM | c391b387af7e8f7903ea18c0435877e8db60e9de667f8a99aba9544508b74fd9 |
| l49 swap | b5a4e02d88fb9a31c6fdd3470fdfa972f65ca36e82c9cde3da77698812085d47 |
| l50 deposit | e37f4dbf5bb986b40519d5507bf6c9df86d182b8c0489fb94ad388e44f5a70cd |

## Short close

| Step | Hash |
|---|---|
| l60 withdraw USDC | c9df16ec2612fede71c4d35d07955573b90479c52a01ecb8eeab69a7a58ce6fd |
| l61 swap USDC→XLM (−4 bps vs oracle) | c2cb9c2165e98c3d957bc43fda01fb064adbba9d3727ad796148fba0c8dfaad8 |
| l62 repay XLM | d7ca4eec8733a6f55639d2f8ab65e16dd3d67335afda8ff7dd10baebbdc330e9 |
| l63 withdraw | b284b05a76335fe0fb0f79d57c79ac37cf1f3b9e04c8cf0463cb01ca13f000e3 |
| l64 swap (19 bps) | 201395e5edf593c670ba043ced0b2b60ef55019d9bbb3bd148f1b75939d0694e |
| l65 repay | 8d9bd9cd57433ebafc5f3eb6aad103b0a0f5a77cf2ad62c936fdf6952bcc79fe |
| l66 withdraw | 9ad8d71672759e43929b5ef2d22fbb8b65f6381dbc483eb8b9cfe729f0e2a1a3 |
| l67 swap (25 bps) | 2c191788fe5cdf3dbabfaaa61607f32be86cae594934369d84ef91bd1da65a1d |
| l68 repay | b8c1ff36c75996418298d0002470e4b32d0ca6dddc14915aec49139247e0def6 |
| l69 repay interest dust | 7e91c73a11275d50748250a7520fcdec489aa7d34fdc246f882827ac345c0577 |
| l70 withdraw all USDC | 43854741722dcef7b20413757d6c41b2ac06d608f49aa2f15cd539c0d6d321b0 |

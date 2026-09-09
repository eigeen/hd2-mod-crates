use super::*;

#[test]
fn resolves_known_resource_semantics_and_preserves_valid_declarations() {
    let cases = [
        (UNIT_ID, POISON_STREAM_SIZE, Some(0), 0),
        (UNIT_ID, 4, Some(16), 0),
        (BONE_ID, POISON_STREAM_SIZE, Some(16), 0),
        (TEX_ID, POISON_STREAM_SIZE, Some(32), 32),
        (TEX_ID, 16, Some(32), 16),
        (0x1234, 16, Some(32), 16),
        (0x1234, POISON_STREAM_SIZE, Some(0), 0),
        (UNIT_ID, POISON_STREAM_SIZE, None, 0),
        (BONE_ID, 4, None, 0),
        (TEX_ID, POISON_STREAM_SIZE, None, 32),
        (TEX_ID, 64, None, 64),
        (0x1234, 64, None, 64),
    ];
    let toc_data = texture_toc(32);
    for (type_id, size, stream_len, expected) in cases {
        let resolved = resolve_stream_size(
            StreamDeclaration {
                type_id,
                size,
                toc_data: &toc_data,
                ..declaration()
            },
            stream_len,
        )
        .unwrap();
        assert_eq!(
            resolved.size, expected,
            "type={type_id}, size={size}, length={stream_len:?}"
        );
        assert_eq!(resolved.repair.is_some(), size != expected);
        if let Some(repair) = resolved.repair {
            assert_eq!(
                (repair.original_size, repair.repaired_size),
                (size, expected)
            );
            assert_eq!(repair.poison_value_detected, size == POISON_STREAM_SIZE);
        }
    }
}

#[test]
fn unknown_length_is_not_an_empty_stream() {
    assert!(resolve_stream_size(declaration(), None).is_err());
    assert_eq!(resolve_stream_size(declaration(), Some(0)).unwrap().size, 0);
}

#[test]
fn rejects_unresolvable_sizes_and_malformed_texture_metadata() {
    let toc_data = texture_toc(32);
    let cases = [
        (
            0x1234,
            &toc_data[..],
            Some(16),
            "no exact stream-size calculator",
        ),
        (TEX_ID, &toc_data[..], Some(31), "still out of bounds"),
        (TEX_ID, &toc_data[..19], Some(32), "too small"),
    ];
    for (type_id, toc_data, length, reason) in cases {
        let error = resolve_stream_size(
            StreamDeclaration {
                type_id,
                toc_data,
                ..declaration()
            },
            length,
        )
        .err()
        .expect("invalid declaration must fail");
        assert!(error.to_string().contains(reason), "{error}");
    }
}

#[test]
fn ignores_unused_offsets_and_checks_overflow_before_slicing() {
    let empty = resolve_stream_size(
        StreamDeclaration {
            offset: u64::MAX,
            size: 0,
            ..declaration()
        },
        Some(16),
    )
    .unwrap();
    assert_eq!(empty.size, 0);
    assert!(empty.repair.is_none());

    let toc_data = texture_toc(32);
    let error = resolve_stream_size(
        StreamDeclaration {
            type_id: TEX_ID,
            offset: u64::MAX,
            toc_data: &toc_data,
            ..declaration()
        },
        Some(32),
    )
    .err()
    .expect("overflow must fail");
    assert!(error.to_string().contains("still out of bounds"));
}

fn texture_toc(size: u32) -> Vec<u8> {
    let mut bytes = vec![0; 20];
    bytes[16..20].copy_from_slice(&size.to_le_bytes());
    bytes
}

fn declaration() -> StreamDeclaration<'static> {
    StreamDeclaration {
        file_id: 42,
        type_id: 0x1234,
        offset: 0,
        size: POISON_STREAM_SIZE,
        toc_data: &[],
    }
}

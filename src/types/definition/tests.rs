use super::*;

    #[test]
    fn test_int_float_display_uses_byte_width() {
        assert_eq!(Type::int().to_string(), "int");
        assert_eq!(Type::float().to_string(), "float");
        assert_eq!(Type::Int { bits: 32, signed: true }.to_string(), "int(4)+");
        assert_eq!(Type::Int { bits: 8, signed: false }.to_string(), "int(1)-");
        assert_eq!(Type::Int { bits: 64, signed: false }.to_string(), "int(8)-");
        assert_eq!(Type::Float { bits: 32 }.to_string(), "float(4)");
    }

    #[test]
    fn test_space_id_unique() {
        let id1 = SpaceId::new();
        let id2 = SpaceId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_path_display() {
        assert_eq!(Path::Root.to_string(), "/");
        assert_eq!(Path::Parent.to_string(), "..");
        assert_eq!(Path::Ident("x".to_string()).to_string(), "x");
        assert_eq!(
            Path::Chain {
                base: Box::new(Path::Ident("a".to_string())),
                segment: "b".to_string()
            }.to_string(),
            "a.b"
        );
    }

    #[test]
    fn test_type_def_name() {
        let class_def = TypeDef::Class {
            name: "TestClass".to_string(),
            fields: vec![],
            methods: vec![],
            generics: vec![],
            parent: None,
        };
        assert_eq!(class_def.name(), "TestClass");
    }

    #[test]
    fn test_region() {
        let mut region = Region::new("test");
        region.bind("x", Binding {
            name: "x".to_string(),
            entity: Entity::Variable {
                ty: Type::i32(),
                initialized: true,
            },
            span: Span::new(0, 1),
            mutable: false,
            visibility: Visibility::Private,
        });

        assert!(region.lookup("x").is_some());
        assert!(region.lookup("y").is_none());
    }

    #[test]
    fn from_str_and_type_from_str_are_the_same_table() {
        assert_eq!(Type::from_str("object").unwrap(), Type::Variadic);
        assert_eq!(type_from_str("object").unwrap(), Type::Variadic);
        assert_eq!(Type::from_str("buf").unwrap(), Type::buf());
        assert_eq!(Type::from_str("i32").unwrap(), Type::Int { bits: 32, signed: true });
        assert!(matches!(
            Type::from_str("List").unwrap(),
            Type::NamedType { name } if name == "List"
        ));
        assert!(matches!(
            Type::from_str("List<int>").unwrap(),
            Type::App { name, args } if name == "List" && args.len() == 1
        ));
        assert_eq!(
            Type::from_str("List<int>").unwrap(),
            type_from_str("List<int>").unwrap()
        );
    }

    #[test]
    fn test_object_c_handle_coercion() {
        let object = Type::Variadic;
        let i64 = Type::int();
        let i32 = Type::i32();
        let s = Type::String;
        let named = Type::NamedType { name: "Foo".to_string() };
        let r = Type::Ref { elem: Box::new(Type::int()), mutable: false };

        assert!(object.can_coerce_from(&i64));
        assert!(object.can_coerce_from(&i32));
        assert!(object.can_coerce_from(&s));
        assert!(object.can_coerce_from(&named));
        assert!(object.can_coerce_from(&r));
        assert!(object.can_coerce_from(&object));
        assert!(!object.can_coerce_from(&Type::Bool));
        assert!(!Type::Bool.can_coerce_from(&object));

        assert!(i64.can_coerce_from(&object));
        assert!(!i32.can_coerce_from(&object));
        assert!(s.can_coerce_from(&object));

        assert!(Type::Int { bits: 64, signed: true }
            .can_coerce_from(&Type::Int { bits: 32, signed: true }));
        assert!(!Type::Int { bits: 32, signed: true }
            .can_coerce_from(&Type::Int { bits: 64, signed: true }));
        assert!(!Type::float().can_coerce_from(&i64));
        assert!(!i64.can_coerce_from(&Type::float()));
    }

//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.2

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "pending_tasks")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        column_type = "Binary(BlobSize::Blob(None))"
    )]
    pub address: Vec<u8>,
    pub created_at: DateTime,
    #[sea_orm(column_type = "JsonBinary")]
    pub source_data: Json,
    #[sea_orm(column_type = "Binary(BlobSize::Blob(None))")]
    pub bytecode: Vec<u8>,
    #[sea_orm(column_type = "Text")]
    pub bytecode_type: String,
    pub submitted: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::bytecode_types::Entity",
        from = "Column::BytecodeType",
        to = "super::bytecode_types::Column::BytecodeType",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    BytecodeTypes,
}

impl Related<super::bytecode_types::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::BytecodeTypes.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}

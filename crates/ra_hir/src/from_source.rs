//! FIXME: write short doc here

use hir_def::{ModuleId, StructId, StructOrUnionId, UnionId};
use hir_expand::name::AsName;
use ra_syntax::{
    ast::{self, AstNode, NameOwner},
    match_ast,
};

use crate::{
    db::{AstDatabase, DefDatabase, HirDatabase},
    ids::{AstItemDef, LocationCtx},
    Const, DefWithBody, Enum, EnumVariant, FieldSource, Function, HasBody, HasSource, ImplBlock,
    Local, Module, ModuleSource, Source, Static, Struct, StructField, Trait, TypeAlias, Union,
    VariantDef,
};

pub trait FromSource: Sized {
    type Ast;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self>;
}

// FIXIME: these two impls are wrong, `ast::StructDef` might produce either a struct or a union
impl FromSource for Struct {
    type Ast = ast::StructDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id: StructOrUnionId = from_source(db, src)?;
        Some(Struct { id: StructId(id) })
    }
}
impl FromSource for Union {
    type Ast = ast::StructDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id: StructOrUnionId = from_source(db, src)?;
        Some(Union { id: UnionId(id) })
    }
}
impl FromSource for Enum {
    type Ast = ast::EnumDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(Enum { id })
    }
}
impl FromSource for Trait {
    type Ast = ast::TraitDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(Trait { id })
    }
}
impl FromSource for Function {
    type Ast = ast::FnDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(Function { id })
    }
}
impl FromSource for Const {
    type Ast = ast::ConstDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(Const { id })
    }
}
impl FromSource for Static {
    type Ast = ast::StaticDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(Static { id })
    }
}
impl FromSource for TypeAlias {
    type Ast = ast::TypeAliasDef;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let id = from_source(db, src)?;
        Some(TypeAlias { id })
    }
}
// FIXME: add impl FromSource for MacroDef

impl FromSource for ImplBlock {
    type Ast = ast::ImplBlock;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let module_src = crate::ModuleSource::from_child_node(
            db,
            src.file_id.original_file(db),
            &src.ast.syntax(),
        );
        let module = Module::from_definition(db, Source { file_id: src.file_id, ast: module_src })?;
        let impls = module.impl_blocks(db);
        impls.into_iter().find(|b| b.source(db) == src)
    }
}

impl FromSource for EnumVariant {
    type Ast = ast::EnumVariant;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let parent_enum = src.ast.parent_enum();
        let src_enum = Source { file_id: src.file_id, ast: parent_enum };
        let variants = Enum::from_source(db, src_enum)?.variants(db);
        variants.into_iter().find(|v| v.source(db) == src)
    }
}

impl FromSource for StructField {
    type Ast = FieldSource;
    fn from_source(db: &(impl DefDatabase + AstDatabase), src: Source<Self::Ast>) -> Option<Self> {
        let variant_def: VariantDef = match src.ast {
            FieldSource::Named(ref field) => {
                let ast = field.syntax().ancestors().find_map(ast::StructDef::cast)?;
                let src = Source { file_id: src.file_id, ast };
                let def = Struct::from_source(db, src)?;
                VariantDef::from(def)
            }
            FieldSource::Pos(ref field) => {
                let ast = field.syntax().ancestors().find_map(ast::EnumVariant::cast)?;
                let src = Source { file_id: src.file_id, ast };
                let def = EnumVariant::from_source(db, src)?;
                VariantDef::from(def)
            }
        };
        variant_def
            .variant_data(db)
            .fields()
            .into_iter()
            .flat_map(|it| it.iter())
            .map(|(id, _)| StructField { parent: variant_def, id })
            .find(|f| f.source(db) == src)
    }
}

impl Local {
    pub fn from_source(db: &impl HirDatabase, src: Source<ast::BindPat>) -> Option<Self> {
        let file_id = src.file_id;
        let parent: DefWithBody = src.ast.syntax().ancestors().find_map(|it| {
            let res = match_ast! {
                match it {
                    ast::ConstDef(ast) => { Const::from_source(db, Source { ast, file_id})?.into() },
                    ast::StaticDef(ast) => { Static::from_source(db, Source { ast, file_id})?.into() },
                    ast::FnDef(ast) => { Function::from_source(db, Source { ast, file_id})?.into() },
                    _ => return None,
                }
            };
            Some(res)
        })?;
        let source_map = parent.body_source_map(db);
        let src = src.map(ast::Pat::from);
        let pat_id = source_map.node_pat(src.as_ref())?;
        Some(Local { parent, pat_id })
    }
}

impl Module {
    pub fn from_declaration(db: &impl DefDatabase, src: Source<ast::Module>) -> Option<Self> {
        let parent_declaration = src.ast.syntax().ancestors().skip(1).find_map(ast::Module::cast);

        let parent_module = match parent_declaration {
            Some(parent_declaration) => {
                let src_parent = Source { file_id: src.file_id, ast: parent_declaration };
                Module::from_declaration(db, src_parent)
            }
            _ => {
                let src_parent = Source {
                    file_id: src.file_id,
                    ast: ModuleSource::new(db, Some(src.file_id.original_file(db)), None),
                };
                Module::from_definition(db, src_parent)
            }
        }?;

        let child_name = src.ast.name()?;
        parent_module.child(db, &child_name.as_name())
    }

    pub fn from_definition(db: &impl DefDatabase, src: Source<ModuleSource>) -> Option<Self> {
        match src.ast {
            ModuleSource::Module(ref module) => {
                assert!(!module.has_semi());
                return Module::from_declaration(
                    db,
                    Source { file_id: src.file_id, ast: module.clone() },
                );
            }
            ModuleSource::SourceFile(_) => (),
        };

        let original_file = src.file_id.original_file(db);

        let (krate, module_id) =
            db.relevant_crates(original_file).iter().find_map(|&crate_id| {
                let crate_def_map = db.crate_def_map(crate_id);
                let local_module_id = crate_def_map.modules_for_file(original_file).next()?;
                Some((crate_id, local_module_id))
            })?;
        Some(Module { id: ModuleId { krate, module_id } })
    }
}

fn from_source<N, DEF>(db: &(impl DefDatabase + AstDatabase), src: Source<N>) -> Option<DEF>
where
    N: AstNode,
    DEF: AstItemDef<N>,
{
    let module_src =
        crate::ModuleSource::from_child_node(db, src.file_id.original_file(db), &src.ast.syntax());
    let module = Module::from_definition(db, Source { file_id: src.file_id, ast: module_src })?;
    let ctx = LocationCtx::new(db, module.id, src.file_id);
    Some(DEF::from_ast(ctx, &src.ast))
}

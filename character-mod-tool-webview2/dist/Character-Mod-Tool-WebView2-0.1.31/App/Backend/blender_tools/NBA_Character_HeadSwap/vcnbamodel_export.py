from .nba2k_character_buffer_export import patch_hihead_model_buffers
from .nba2k_vcz import expand_scene_buffers
try:
    from .nba2k_character_eyeball_tool import patch_eye_files_for_export
except Exception:
    patch_eye_files_for_export = None
import bpy
from bpy.types import Operator
from bpy.props import StringProperty, BoolProperty, EnumProperty
from bpy_extras.io_utils import ExportHelper
import os
import tempfile
import zipfile
import shutil
import traceback

# Define enum items for the dropdown
COMPILE_TARGETS = \
[
    ("option_vcexport_auto", "Auto (NBA 2K26 / NBA 2K27)", "Detect the target from the original IFF"),
    ("option_vcexport_26", "NBA 2K26", "Use an NBA 2K26 original IFF"),
    ("option_vcexport_27", "NBA 2K27", "Use an NBA 2K27 original IFF"),
]

class vCNBACharacterExport(Operator, ExportHelper):
    """Export Visual Concepts | NBA2K Character HIHEAD"""
    bl_idname = "export_vcnbacharacter.save"
    bl_label = "Export HIHEAD + Model"
    filename_ext = ".iff"

    filter_glob: StringProperty(
        default="*.iff;*.IFF",
        options={'HIDDEN'},
        maxlen=255,
    )

    source_iff_path: StringProperty(
        name="Original IFF",
        description="Original full IFF archive to copy untouched files from before inserting the corrected model buffers",
        default="",
    )

    export_object_name: StringProperty(
        name="Export Mesh",
        description="Mesh object captured when the export window opened",
        default="",
    )

    compile_target: EnumProperty(
        name="",
        items=COMPILE_TARGETS,
        default="option_vcexport_auto",
        description="Select target SCNE format",
    )
    
    keep_backup: BoolProperty(
        name="Keep Backup",
        description="Keep backup of original .iff file in backup folder",
        default=True,
    )

    include_eye_tool_data: BoolProperty(
        name="Include Eye Tool Data",
        description="Insert solved NBA Character Eye Tool SCNE/mathnodes changes into the exported IFF",
        default=True,
    )

    auto_source_iff: BoolProperty(
        name="Auto-fill Original IFF from selected mesh",
        description="Use the active/selected imported mesh to choose the Original IFF automatically",
        default=True,
    )

    def extract_iff(self, iff_path):
        """Extract .iff archive to temporary directory"""
        print(f"\n[IFF Export] Extracting IFF: {iff_path}")
        
        temp_dir = tempfile.mkdtemp(prefix="nba2k_iff_export_")
        
        try:
            with zipfile.ZipFile(iff_path, 'r') as zip_ref:
                names = zip_ref.namelist()
                for member in names:
                    if '..' in member or member.startswith(('/', '\\')):
                        print(f"[IFF Export] Skipped unsafe archive member: {member}")
                        continue
                    zip_ref.extract(member, temp_dir)
                print(f"[IFF Export] Extracted {len(names)} files to temp")
            
            # Find the character head/body scene file.
            scne_file = None
            for root, dirs, files in os.walk(temp_dir):
                for file in files:
                    if file.lower() == 'hihead.scne':
                        scne_file = os.path.join(root, file)
                        break
                if scne_file:
                    break

            if scne_file:
                vcz_result = expand_scene_buffers(temp_dir, scne_file, iff_path)
                self._source_game = vcz_result.get("game", "")
                if vcz_result.get("decoded") or vcz_result.get("shared"):
                    print(
                        f"[{self._source_game or 'NBA Character'} Export] Expanded "
                        f"{vcz_result['decoded']} embedded VCZ buffer(s) and "
                        f"{vcz_result['shared']} shared manifest buffer(s)."
                    )
                print(
                    f"[NBA Character Export] Detected {self._source_game or 'character'} "
                    f"positions: {vcz_result.get('position_format', 'unknown')}."
                )
            
            return scne_file, temp_dir
            
        except Exception as e:
            print(f"[IFF Export] ERROR extracting: {e}")
            shutil.rmtree(temp_dir)
            return None, None

    def repack_iff(self, temp_dir, output_iff_path, source_iff_path=None):
        """Repack directory contents back into .iff file"""
        print(f"\n[IFF Export] Repacking to: {output_iff_path}")
        self._last_repack_error = ""
        self._last_repack_output_path = output_iff_path
        tmp_output_path = None

        try:
            output_dir = os.path.dirname(output_iff_path) or os.getcwd()
            os.makedirs(output_dir, exist_ok=True)

            # Create backup in backup folder if requested
            if self.keep_backup and os.path.exists(output_iff_path):
                try:
                    iff_name = os.path.basename(output_iff_path)
                    backup_folder = os.path.join(output_dir, "backup")
                    os.makedirs(backup_folder, exist_ok=True)

                    backup_path = os.path.join(backup_folder, iff_name)
                    shutil.copy2(output_iff_path, backup_path)
                    print(f"[IFF Export] Created backup: {backup_path}")
                except Exception as backup_error:
                    print(f"[IFF Export] Warning: could not create backup: {backup_error}")

            output_name = os.path.basename(output_iff_path) or "export.iff"
            tmp_fd, tmp_output_path = tempfile.mkstemp(prefix="nba_char_export_", suffix=".iff")
            os.close(tmp_fd)
            os.remove(tmp_output_path)

            # Build the archive completely before touching the final IFF.
            with zipfile.ZipFile(tmp_output_path, 'w', zipfile.ZIP_DEFLATED) as zipf:
                for root, dirs, files in os.walk(temp_dir):
                    for file in files:
                        file_path = os.path.join(root, file)
                        arcname = os.path.relpath(file_path, temp_dir).replace(os.sep, '/')
                        zipf.write(file_path, arcname)
                        print(f"[IFF Export] Added: {arcname}")

            try:
                os.replace(tmp_output_path, output_iff_path)
            except PermissionError as move_error:
                fallback_dir = os.path.join(os.path.expanduser("~"), "Documents", "NBA_Character_HeadSwap_Exports")
                os.makedirs(fallback_dir, exist_ok=True)
                fallback_path = os.path.join(fallback_dir, output_name)
                os.replace(tmp_output_path, fallback_path)
                self._last_repack_output_path = fallback_path
                self._last_repack_error = f"Access denied to selected folder; saved to {fallback_path}"
                print(f"[IFF Export] Warning: {move_error}")
                print(f"[IFF Export] Saved fallback IFF: {fallback_path}")
            print(f"[IFF Export] Successfully repacked IFF")
            return True

        except Exception as e:
            self._last_repack_error = str(e)
            print(f"[IFF Export] ERROR repacking: {e}")
            traceback.print_exc()
            if tmp_output_path and os.path.exists(tmp_output_path):
                try:
                    os.remove(tmp_output_path)
                except Exception as cleanup_error:
                    print(f"[IFF Export] Warning: could not remove temp output: {cleanup_error}")
            return False

    def _context_selected_character_meshes(self):
        selected_meshes = [obj for obj in bpy.context.selected_objects if getattr(obj, "type", None) == 'MESH']
        if selected_meshes:
            active = getattr(bpy.context, "active_object", None)
            if active in selected_meshes:
                return [active]
            return [selected_meshes[0]]
        active = getattr(bpy.context, "active_object", None)
        if active and getattr(active, "type", None) == 'MESH':
            return [active]
        candidate = bpy.data.objects.get("hihead")
        if candidate and getattr(candidate, "type", None) == 'MESH':
            return [candidate]
        return []

    def _selected_character_meshes(self):
        locked_name = getattr(self, "export_object_name", "") if self is not None else ""
        if locked_name:
            locked = bpy.data.objects.get(locked_name)
            if locked and getattr(locked, "type", None) == 'MESH':
                return [locked]
        return self._context_selected_character_meshes() if self is not None else vCNBACharacterExport._context_selected_character_meshes(None)

    def _expand_character_meshes(self, selected_meshes):
        meshes = list(selected_meshes)
        return sorted(
            {obj.name: obj for obj in meshes}.values(),
            key=lambda obj: (int(obj.get("nba2k_global_vertex_offset", 0) or 0), obj.name),
        )

    def _source_path_from_scene(self, export_meshes):
        for obj in export_meshes:
            source = obj.get("nba2k_source_path", "")
            if source and os.path.exists(source):
                return source
        scene = bpy.context.scene
        custom_source = scene.get("nba_character_source_path", "")
        if custom_source and os.path.exists(custom_source):
            return custom_source
        for attr in ("nbachar_eye_scne_or_iff",):
            source = getattr(scene, attr, "")
            if source and os.path.exists(source):
                return source
        return ""

    def _selected_source_iff(self, selected_meshes):
        source_path = self._source_path_from_scene(selected_meshes)
        if source_path and source_path.lower().endswith('.iff') and os.path.exists(source_path):
            return source_path
        return ""

    def _scene_source_iff_override(self):
        try:
            source_path = getattr(bpy.context.scene, "nbachar_export_original_iff", "")
        except Exception:
            source_path = ""
        if source_path and source_path.lower().endswith('.iff') and os.path.exists(source_path):
            return source_path
        return ""

    def _source_iff_for_export(self, filepath, export_meshes):
        source_path = self.source_iff_path
        if source_path and source_path.lower().endswith('.iff') and os.path.exists(source_path):
            return source_path
        source_path = self._source_path_from_scene(export_meshes)
        if source_path and source_path.lower().endswith('.iff') and os.path.exists(source_path):
            return source_path
        if filepath.lower().endswith('.iff') and os.path.exists(filepath):
            return filepath
        return ""

    def _mesh_source_matches_archive(self, obj, source_iff_path):
        source = obj.get("nba2k_source_path", "")
        if not source:
            return False
        try:
            return os.path.normcase(os.path.abspath(source)) == os.path.normcase(os.path.abspath(source_iff_path))
        except Exception:
            return False

    def _filter_meshes_for_source_archive(self, export_meshes, source_iff_path):
        if not source_iff_path:
            return export_meshes
        matched = [obj for obj in export_meshes if self._mesh_source_matches_archive(obj, source_iff_path)]
        if matched:
            skipped = [obj.name for obj in export_meshes if obj not in matched]
            if skipped:
                print(
                    "[Export] Skipping mesh(es) from a different source archive: "
                    + ", ".join(skipped)
                )
            return matched
        return export_meshes

    def _normalize_iff_output_path(self, filepath, source_iff_path):
        output_dir = os.path.dirname(filepath) or os.path.dirname(source_iff_path) or os.getcwd()
        output_name = os.path.basename(filepath)
        if not output_name or output_name.lower() == ".iff":
            output_name = os.path.basename(source_iff_path)
        elif not output_name.lower().endswith(".iff"):
            output_name = os.path.splitext(output_name)[0] + ".iff"
        return os.path.join(output_dir, output_name)

    def _export_existing_hihead_payload(self, scne_path, export_meshes):
        print(f"\n[Export] Updating model buffers beside: {scne_path}")
        payload = patch_hihead_model_buffers(scne_path, export_meshes)
        print(f"[Export] Included scene: {payload['scne']}")
        print(
            f"[Export] Updated model buffer: {payload['position_buffer']} "
            f"({payload['vertex_count']} Blender vertices into {payload['buffer_vertex_count']} buffer vertices)"
        )
        print(f"[Export] Position format: {payload['position_format']}")
        if payload.get("lod_buffers"):
            print(f"[Export] Updated packed lower LOD buffers: {len(payload['lod_buffers'])}")
        if payload.get("uv_buffer"):
            print(f"[Export] Updated UV buffer: {payload['uv_buffer']}")
        else:
            print("[Export] No UV buffer update was needed")
        return payload

    def doExport(self, filepath, keywords):
        """Main export logic with .iff support"""
        selected_meshes = self._selected_character_meshes()
        if not selected_meshes:
            self.report({'ERROR'}, "Select the character mesh object(s) to export.")
            return {'CANCELLED'}
        export_meshes = self._expand_character_meshes(selected_meshes)
        if not any(obj.get("nba2k_scne_top_prim_groups_json") or obj.get("nba2k_vertex_components") for obj in export_meshes):
            self.report({'ERROR'}, "Selected mesh has no NBA Character import metadata. Re-import with v1.10+ before exporting.")
            return {'CANCELLED'}
        
        temp_dir = None
        scne_path = filepath

        print("\n" + "="*60)
        print("IFF FILE DETECTED - FULL ARCHIVE EXPORT MODE")
        print("="*60)
        selected_source_iff = self._selected_source_iff(selected_meshes)
        if self.auto_source_iff and selected_source_iff:
            source_iff_path = selected_source_iff
            self.source_iff_path = selected_source_iff
        else:
            source_iff_path = self.source_iff_path if self.source_iff_path else self._scene_source_iff_override()
            if not source_iff_path:
                source_iff_path = self._source_iff_for_export(filepath, selected_meshes)
            if not source_iff_path:
                source_iff_path = self._source_iff_for_export(filepath, export_meshes)
        if not source_iff_path:
            self.report({'ERROR'}, "Choose the original full IFF in the Original IFF field, then export to a new .iff.")
            return {'CANCELLED'}
        export_meshes = self._filter_meshes_for_source_archive(export_meshes, source_iff_path)
        filepath = self._normalize_iff_output_path(filepath, source_iff_path)
        is_iff = filepath.lower().endswith('.iff')
        scne_path = filepath
        print(f"[IFF Export] Output path adjusted to: {filepath}")
        print(f"[IFF Export] Source archive: {source_iff_path}")
        print(f"[IFF Export] Output archive: {filepath}")
        scne_path, temp_dir = self.extract_iff(source_iff_path)
        if not scne_path:
            self.report({'ERROR'}, "Failed to extract source IFF or find hihead.SCNE")
            return {'CANCELLED'}
        
        if not os.path.exists(scne_path):
            source_path = self._source_path_from_scene(export_meshes)
            if source_path and source_path.lower().endswith('.scne') and os.path.exists(source_path):
                os.makedirs(os.path.dirname(filepath), exist_ok=True)
                shutil.copy2(source_path, filepath)
                scne_path = filepath
            else:
                self.report({'ERROR'}, "Export needs an existing hihead.SCNE/IFF to patch. Re-import from the original file first.")
                return {'CANCELLED'}

        try:
            self._export_existing_hihead_payload(scne_path, export_meshes)
        except Exception as e:
            print(f"[Export] Existing HIHEAD/model buffer update failed: {e}")
            if temp_dir:
                try:
                    shutil.rmtree(temp_dir)
                except Exception:
                    pass
            self.report({'ERROR'}, f"Could not update hihead model buffers: {e}")
            return {'CANCELLED'}

        if self.include_eye_tool_data and patch_eye_files_for_export:
            eye_ok, eye_msg = patch_eye_files_for_export(bpy.context, scne_path, require_solved=False)
            print(f"[NBA Character Eye Tool] Export integration: {eye_msg}")
            if not eye_ok and not str(eye_msg).startswith("Eye tool data skipped"):
                if temp_dir:
                    try:
                        shutil.rmtree(temp_dir)
                    except Exception:
                        pass
                self.report({'ERROR'}, eye_msg)
                return {'CANCELLED'}
        
        # If working with IFF, repack it
        if is_iff and temp_dir:
            success = self.repack_iff(temp_dir, filepath, source_iff_path)
            
            # Cleanup temp directory
            print(f"\n[Cleanup] Removing temp files")
            try:
                shutil.rmtree(temp_dir)
            except Exception as e:
                print(f"[Cleanup] Warning: {e}")
            
            if not success:
                detail = getattr(self, "_last_repack_error", "")
                if detail:
                    self.report({'ERROR'}, f"Failed to repack IFF: {detail}")
                else:
                    self.report({'ERROR'}, "Failed to repack IFF")
                return {'CANCELLED'}
            
            print("\n" + "="*60)
            print("IFF EXPORT COMPLETE!")
            print("="*60)
            saved_path = getattr(self, "_last_repack_output_path", filepath)
            if os.path.normcase(os.path.abspath(saved_path)) != os.path.normcase(os.path.abspath(filepath)):
                self.report({'INFO'}, f"Exported IFF to safe folder: {saved_path}")
            else:
                self.report({'INFO'}, "Exported hihead.SCNE + model buffers into IFF.")
        else:
            self.report({'INFO'}, "Exported hihead.SCNE + model buffers.")
        
        return {'FINISHED'}

    def draw(self, context):
        layout = self.layout
        box = layout.box()
        
        box.label(text="Game Options:")
        split = box.split(factor=0.2) 
        split.label(text="Target:")
        split.prop(self, "compile_target")

        box2 = layout.box()
        box2.label(text="IFF Options:")
        locked_name = getattr(self, "export_object_name", "")
        if locked_name:
            box2.label(text="Export Mesh: %s" % locked_name)
        box2.prop(self, "auto_source_iff")
        box2.prop(self, "source_iff_path")
        box2.prop(self, "include_eye_tool_data")
        box2.prop(self, "keep_backup")

    def invoke(self, context, event):
        selected_meshes = self._context_selected_character_meshes()
        self.export_object_name = selected_meshes[0].name if selected_meshes else ""
        export_meshes = self._expand_character_meshes(selected_meshes) if selected_meshes else []
        if self.auto_source_iff and selected_meshes:
            source_path = self._selected_source_iff(selected_meshes)
            if source_path:
                self.source_iff_path = source_path
        elif not self.source_iff_path and export_meshes:
            source_path = self._scene_source_iff_override() or self._source_path_from_scene(selected_meshes) or self._source_path_from_scene(export_meshes)
            if source_path and source_path.lower().endswith('.iff') and os.path.exists(source_path):
                self.source_iff_path = source_path
        if self.source_iff_path and not os.path.basename(self.filepath):
            self.filepath = os.path.join(os.path.dirname(self.source_iff_path), os.path.basename(self.source_iff_path))
        context.window_manager.fileselect_add(self)
        return {'RUNNING_MODAL'}
        
    def execute(self, context):
        keywords = self.as_keywords(
            ignore=(
                "axis_forward",
                "axis_up",
                "global_scale",
                "check_existing",
                "filter_glob",
            ),
        )
        return self.doExport(self.filepath, keywords)
    
def menu_bar_vc_export(self, context):
    self.layout.operator(vCNBACharacterExport.bl_idname, text="Visual Concepts Character HIHEAD + Model (.SCNE/.IFF)", icon='USER')

def register_character_export():
    bpy.utils.register_class(vCNBACharacterExport)
    bpy.types.TOPBAR_MT_file_export.append(menu_bar_vc_export)
    return

def unregister_character_export():
    bpy.utils.unregister_class(vCNBACharacterExport)
    bpy.types.TOPBAR_MT_file_export.remove(menu_bar_vc_export)
    return

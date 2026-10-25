"""Headless geometric before/after evidence; never opens or writes game assets."""
import sys
from pathlib import Path
import bpy
from mathutils import Vector

args=sys.argv[sys.argv.index('--')+1:]
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root.parent/'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend'))
from hair_tools.regular_hair_tangent_fit import _load_mesh,_triangles

bpy.ops.wm.read_factory_settings(use_empty=True)
scene=bpy.context.scene
scene.render.engine='CYCLES'
scene.cycles.device='CPU'
scene.cycles.samples=16
scene.render.resolution_x=720
scene.render.resolution_y=720
scene.render.resolution_percentage=100
scene.world=bpy.data.worlds.new('World')
scene.world.use_nodes=True
scene.world.node_tree.nodes['Background'].inputs[0].default_value=(.09,.09,.09,1)
scene.world.node_tree.nodes['Background'].inputs[1].default_value=.5
bpy.ops.object.camera_add(location=(40,-140,63))
camera=bpy.context.object
camera.rotation_euler=(Vector((0,0,43))-camera.location).to_track_quat('-Z','Y').to_euler()
camera.data.type='ORTHO'
camera.data.ortho_scale=65
scene.camera=camera
bpy.ops.object.light_add(type='AREA',location=(-50,-60,110))
bpy.context.object.data.energy=150000
bpy.context.object.data.shape='DISK'
bpy.context.object.data.size=70
bpy.context.object.rotation_euler=(Vector((0,0,40))-bpy.context.object.location).to_track_quat('-Z','Y').to_euler()
material=bpy.data.materials.new('Neutral Clay')
material.diffuse_color=(.55,.55,.55,1)
for source,dest in ((args[0],args[2]+'-before.png'),(args[1],args[2]+'-after.png')):
    m=_load_mesh(source,True)
    triangles=sum((_triangles(m,name) for name in ('face_shader','arms_shader','torso_shader')),[])
    mesh=bpy.data.meshes.new('Player')
    mesh.from_pydata([(x,-z,y) for x,y,z in m.positions],[],triangles)
    mesh.materials.append(material)
    obj=bpy.data.objects.new('Player',mesh)
    scene.collection.objects.link(obj)
    for polygon in mesh.polygons: polygon.use_smooth=True
    scene.render.filepath=str(Path(dest).resolve())
    bpy.ops.render.render(write_still=True)
    bpy.data.objects.remove(obj,do_unlink=True)
    bpy.data.meshes.remove(mesh)

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { Boxes, Copy, HardDrive, Plus, Search, Trash2, Wrench, X } from 'lucide-react';
import { useToast } from '../contexts/ToastContext';
import { OpsPageShell } from './layout/OpsPageShell';
import { LightweightHostPanel } from './lightweight/LightweightHostPanel';
import { AppModal } from './shared/AppModal';
import { UsbWriteModal, UsbWriteSelection } from './usb/UsbWriteModal';

interface Image {
  id: string;
  name: string;
  description: string;
  os_type: string;
  os_version: string;
  os_architecture: string;
  os_language: string;
  license_type: string;
  status: string;
  created_at: string;
  updated_at: string;
  size_bytes?: number;
  iso_path?: string;
  has_saved_wizard_state?: boolean;
}

interface ImageManagerProps {
  onBack: () => void;
  onStartWizard?: (imageId?: string | null) => void | Promise<void>;
}

export function ImageManager({ onBack, onStartWizard }: ImageManagerProps) {
  const { showToast } = useToast();
  const [images, setImages] = useState<Image[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedImage, setSelectedImage] = useState<Image | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [showDeleteConfirm, setShowDeleteConfirm] = useState<string | null>(null);
  const [showIsoUsbModal, setShowIsoUsbModal] = useState(false);

  useEffect(() => {
    loadImages();
  }, []);

  const loadImages = async () => {
    try {
      setLoading(true);
      const data = await invoke<Image[]>('list_images');
      setImages(data);
    } catch (err) {
      console.error('Failed to load images:', err);
      showToast('Failed to load images', 'error');
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (imageId: string) => {
    try {
      await invoke('delete_image', { imageId });
      setImages((prev) => prev.filter((img) => img.id !== imageId));
      setShowDeleteConfirm(null);
      if (selectedImage?.id === imageId) {
        setSelectedImage(null);
      }
      showToast('Image deleted', 'success');
    } catch (err) {
      console.error('Failed to delete image:', err);
      showToast('Failed to delete image', 'error');
    }
  };

  const handleDuplicate = async (image: Image) => {
    try {
      const duplicated = await invoke<Image>('duplicate_image', { imageId: image.id });
      setImages((prev) => [duplicated, ...prev]);
      showToast('Image duplicated', 'success');
    } catch (err) {
      console.error('Failed to duplicate image:', err);
      showToast('Failed to duplicate image', 'error');
    }
  };

  const handleBuild = async (image: Image) => {
    console.log('Build image requested from manager:', image.id);
    if (onStartWizard) {
      onStartWizard(image.id);
    } else {
      showToast('Wizard navigation is unavailable in this view', 'warning');
    }
  };

  const handleWriteIsoToUsb = async (selection: UsbWriteSelection) => {
    if (!selectedImage?.iso_path) {
      throw new Error('No ISO path available for this image.');
    }

    const summary = await invoke<string>('write_iso_to_usb', {
      request: {
        isoPath: selectedImage.iso_path,
        targetDiskNumber: selection.targetDiskNumber,
        confirmationToken: selection.confirmationToken,
      },
    });

    showToast(summary, 'success');
    setShowIsoUsbModal(false);
  };

  const formatDate = (isoString: string) => {
    return new Date(isoString).toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  };

  const formatBytes = (bytes?: number) => {
    if (!bytes) return '-';
    const gb = bytes / (1024 * 1024 * 1024);
    return `${gb.toFixed(2)} GB`;
  };

  const getStatusClass = (status: string) => {
    switch (status) {
      case 'Ready':
        return 'ops-pill ops-pill-ready';
      case 'Draft':
        return 'ops-pill ops-pill-draft';
      case 'Building':
        return 'ops-pill ops-pill-building';
      case 'Error':
        return 'ops-pill ops-pill-error';
      default:
        return 'ops-pill ops-pill-draft';
    }
  };

  const filteredImages = images.filter(
    (image) =>
      image.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
      image.description.toLowerCase().includes(searchTerm.toLowerCase())
  );

  if (loading) {
    return (
      <div className="ops-loading-screen">
        <div className="ops-spinner" />
        <p>Loading images...</p>
      </div>
    );
  }

  return (
    <>
      <OpsPageShell
        kicker="Image Library"
        title="Image Manager"
        subtitle="Review and organize deployment images with fast duplicate and cleanup actions."
        onBack={onBack}
        headerActions={
          onStartWizard ? (
            <button type="button" onClick={() => onStartWizard?.()} className="ops-btn ops-btn-primary">
              <Plus size={15} />
              <span>Create New Image</span>
            </button>
          ) : undefined
        }
      >
        <div className="ops-layout-stack">
          <LightweightHostPanel
            description="Control the embedded BitOSDT lightweight PXE host outside the build wizard."
            helperText="Use Start Host when you want to serve the staged simple-mode PXE files. If the staging folder is empty, run a simple-mode build first."
          />

          <section className="ops-card">
            <div className="ops-toolbar">
              <label className="ops-search-wrap">
                <Search size={15} className="ops-search-icon" />
                <input
                  type="text"
                  placeholder="Search images..."
                  value={searchTerm}
                  onChange={(event) => setSearchTerm(event.target.value)}
                  className="ops-input ops-input-search"
                />
              </label>
              <span className="ops-meta-pill">
                {filteredImages.length} {filteredImages.length === 1 ? 'image' : 'images'}
              </span>
            </div>
          </section>

          {filteredImages.length === 0 ? (
            <section className="ops-card ops-empty-state">
              <span className="ops-empty-icon">
                <Boxes size={26} />
              </span>
              <h2 className="ops-card-title">No images found</h2>
              <p className="ops-card-subtitle">
                Images created with the wizard will appear here for duplication, review, and deletion.
              </p>
              {onStartWizard ? (
                <button type="button" onClick={() => onStartWizard?.()} className="ops-btn ops-btn-primary">
                  <Plus size={15} />
                  <span>Create Your First Image</span>
                </button>
              ) : null}
            </section>
          ) : (
            <section className="ops-image-grid">
              {filteredImages.map((image) => (
                <article key={image.id} className="ops-image-card" onClick={() => setSelectedImage(image)}>
                  <div className="ops-image-card-head">
                    <div>
                      <h3 className="ops-image-title">{image.name}</h3>
                      <p className="ops-image-subtitle">
                        {image.os_type} {image.os_version}
                      </p>
                    </div>
                    <span className={getStatusClass(image.status)}>{image.status}</span>
                  </div>

                  <p className="ops-image-description">{image.description || 'No description provided.'}</p>

                  <div className="ops-image-meta">
                    <p>
                      <span>Architecture</span>
                      <strong>{image.os_architecture}</strong>
                    </p>
                    <p>
                      <span>Language</span>
                      <strong>{image.os_language}</strong>
                    </p>
                    <p>
                      <span>License</span>
                      <strong>{image.license_type}</strong>
                    </p>
                    <p>
                      <span>Size</span>
                      <strong>{formatBytes(image.size_bytes)}</strong>
                    </p>
                  </div>

                  <div className="ops-image-footer">
                    <span>Created {formatDate(image.created_at)}</span>
                    <div className="ops-icon-actions">
                      <button
                        type="button"
                        className="ops-icon-btn"
                        title="Duplicate"
                        onClick={(event) => {
                          event.stopPropagation();
                          handleDuplicate(image);
                        }}
                      >
                        <Copy size={15} />
                      </button>
                      <button
                        type="button"
                        className="ops-icon-btn ops-icon-btn-danger"
                        title="Delete"
                        onClick={(event) => {
                          event.stopPropagation();
                          setShowDeleteConfirm(image.id);
                        }}
                      >
                        <Trash2 size={15} />
                      </button>
                    </div>
                  </div>
                </article>
              ))}
            </section>
          )}
        </div>
      </OpsPageShell>

      {selectedImage ? (
        <AppModal open onClose={() => setSelectedImage(null)} labelledBy="image-details-title">
          <>
            <div className="ops-modal-head">
              <div>
                <h2 id="image-details-title">{selectedImage.name}</h2>
                <p>{selectedImage.description || 'No description provided.'}</p>
              </div>
              <button type="button" className="ops-btn ops-btn-ghost" onClick={() => setSelectedImage(null)} aria-label="Close">
                <X size={15} />
                <span>Close</span>
              </button>
            </div>

            <div className="ops-modal-body">
              <div className="ops-detail-grid">
                <div>
                  <span>Operating System</span>
                  <strong>
                    {selectedImage.os_type} {selectedImage.os_version}
                  </strong>
                </div>
                <div>
                  <span>Architecture</span>
                  <strong>{selectedImage.os_architecture}</strong>
                </div>
                <div>
                  <span>Language</span>
                  <strong>{selectedImage.os_language}</strong>
                </div>
                <div>
                  <span>License</span>
                  <strong>{selectedImage.license_type}</strong>
                </div>
              </div>

              <div className="ops-detail-row">
                <span>Status</span>
                <span className={getStatusClass(selectedImage.status)}>{selectedImage.status}</span>
              </div>

              {selectedImage.iso_path ? (
                <div className="ops-detail-row">
                  <span>ISO Path</span>
                  <strong className="ops-break">{selectedImage.iso_path}</strong>
                </div>
              ) : null}
              {!selectedImage.has_saved_wizard_state ? (
                <div className="ops-detail-row">
                  <span>Edit State</span>
                  <strong className="ops-break text-amber-700">
                    Legacy profile: missing full wizard state, defaults may be applied during edit.
                  </strong>
                </div>
              ) : null}
            </div>

            <div className="ops-modal-foot">
              <button type="button" onClick={() => handleBuild(selectedImage)} className="ops-btn ops-btn-primary">
                <Wrench size={15} />
                <span>Edit Image</span>
              </button>
              {selectedImage.iso_path ? (
                <button
                  type="button"
                  onClick={() => setShowIsoUsbModal(true)}
                  className="ops-btn ops-btn-secondary"
                >
                  <HardDrive size={15} />
                  <span>Write ISO to USB</span>
                </button>
              ) : null}
              <button type="button" onClick={() => handleDuplicate(selectedImage)} className="ops-btn ops-btn-secondary">
                <Copy size={15} />
                <span>Duplicate</span>
              </button>
              <button
                type="button"
                onClick={() => setShowDeleteConfirm(selectedImage.id)}
                className="ops-btn ops-btn-danger"
              >
                <Trash2 size={15} />
                <span>Delete</span>
              </button>
              <button type="button" onClick={() => setSelectedImage(null)} className="ops-btn ops-btn-ghost">
                Close
              </button>
            </div>
          </>
        </AppModal>
      ) : null}

      {showDeleteConfirm ? (
        <AppModal open onClose={() => setShowDeleteConfirm(null)} size="compact" labelledBy="delete-image-title">
          <>
            <div className="ops-modal-head">
              <div>
                <h2 id="delete-image-title">Delete Image</h2>
                <p>This action cannot be undone.</p>
              </div>
            </div>
            <div className="ops-modal-body">
              <p>Are you sure you want to delete this image profile?</p>
            </div>
            <div className="ops-modal-foot">
              <button type="button" onClick={() => setShowDeleteConfirm(null)} className="ops-btn ops-btn-ghost">
                Cancel
              </button>
              <button type="button" onClick={() => handleDelete(showDeleteConfirm)} className="ops-btn ops-btn-danger">
                <Trash2 size={15} />
                <span>Delete</span>
              </button>
            </div>
          </>
        </AppModal>
      ) : null}

      <UsbWriteModal
        isOpen={showIsoUsbModal}
        title="Write ISO to USB"
        subtitle="Select the removable disk. BitOSDT will wipe the entire disk before writing the ISO image."
        confirmButtonLabel="Write ISO"
        onClose={() => setShowIsoUsbModal(false)}
        onConfirm={handleWriteIsoToUsb}
      />
    </>
  );
}
